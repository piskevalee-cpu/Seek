//! Startpage: the Google index behind a privacy frontend (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/startpage.ts`.
//!
//! Startpage serves its results without any JavaScript, but its defence keys
//! on the requests that skip the handshake with the homepage: the search form
//! carries a session token, and a form with a stale token is sent to the
//! CAPTCHA page. The flow is the one of a browser: read the homepage, take
//! the hidden fields of the form, and post them back with the query.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::html::{elements_with_class, hidden_inputs};
use crate::websearch::query::format_scraper_query;
use crate::websearch::types::{
    Recency, SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The homepage, where the form of the search lives.
const HOME_URL: &str = "https://www.startpage.com/";
/// The address the search form posts to.
const SEARCH_URL: &str = "https://www.startpage.com/sp/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The recency of a search, as the `with_date` field of the form.
fn recency_to_with_date(recency: Recency) -> &'static str {
    match recency {
        Recency::Day => "d",
        Recency::Week => "w",
        Recency::Month => "m",
        Recency::Year => "y",
    }
}

/// The headers of a browser.
fn headers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like \
             Gecko) Chrome/149.0.0.0 Safari/537.36",
        ),
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
        ("Accept-Language", "en-US,en;q=0.9"),
    ]
}

/// Whether the page is the challenge of the bot defence instead of the results.
fn is_challenge(url: &str, html: &str) -> bool {
    url.contains("/errors/")
        || url.contains("/captcha/")
        || url.contains("/sp/captcha")
        || html.contains("component---src-pages-captcha")
        || html.contains("/sp/captcha")
        // The front now serves an Anubis proof-of-work challenge.
        || html.contains("anubis_challenge")
        || html.contains("anubis_version")
}

/// The hidden fields of the search form of the homepage.
///
/// Without the `sc` token the form is useless, so the caller falls back to a
/// plain GET instead of posting a doomed form.
fn parse_form_inputs(html: &str) -> Option<Vec<(String, String)>> {
    for form in elements_with_class(html, "form", "") {
        if form.attribute("action") != Some("/sp/search") {
            continue;
        }
        let inputs: Vec<(String, String)> = hidden_inputs(&form.inner)
            .into_iter()
            .filter(|(name, _)| name != "query")
            .collect();
        if inputs.iter().any(|(name, _)| name == "sc") {
            return Some(inputs);
        }
    }
    None
}

/// The address of a result, when it points away from Startpage.
fn sanitize_result_url(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }
    let absolute = if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else {
        let rest = href.strip_prefix("//")?;
        format!("https://{rest}")
    };
    let host = absolute
        .split("://")
        .nth(1)?
        .split('/')
        .next()?
        .to_lowercase();
    if host == "startpage.com" || host.ends_with(".startpage.com") {
        return None;
    }
    Some(absolute)
}

/// The results of a page of Startpage, in the order of the page.
fn parse_results(html: &str) -> Vec<SearchSource> {
    let mut results: Vec<SearchSource> = Vec::new();
    for block in elements_with_class(html, "div", "result") {
        let Some(anchor) = elements_with_class(&block.inner, "a", "result-link")
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(url) = anchor.attribute("href").and_then(sanitize_result_url) else {
            continue;
        };
        // The title is in a heading of the anchor.
        let heading = elements_with_class(&anchor.inner, "h2", "")
            .into_iter()
            .next()
            .or_else(|| {
                elements_with_class(&anchor.inner, "h3", "")
                    .into_iter()
                    .next()
            });
        let title = heading
            .map(|heading| crate::websearch::html::text_of(&heading.inner))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| crate::websearch::html::text_of(&anchor.inner));
        if title.is_empty() {
            continue;
        }
        let snippet = elements_with_class(&block.inner, "p", "description")
            .into_iter()
            .next()
            .map(|element| crate::websearch::html::text_of(&element.inner))
            .filter(|text| !text.is_empty());
        results.push(SearchSource {
            title,
            url,
            snippet,
            ..Default::default()
        });
    }
    results
}

/// The Startpage search provider.
pub struct Startpage;

impl SearchProvider for Startpage {
    fn id(&self) -> &'static str {
        "startpage"
    }

    fn label(&self) -> &'static str {
        "Startpage"
    }

    fn is_available(&self, _params: &SearchParams) -> bool {
        true
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            // Startpage proxies Google, so the operators work as they are.
            let query = format_scraper_query(
                &params.query,
                Some(&params.parsed),
                &crate::websearch::query::GOOGLE_QUERY_SYNTAX,
            );

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("startpage", format!("the client failed: {error}"))
                })?;

            // The handshake with the homepage: the token of the form.
            let mut fields: Option<Vec<(String, String)>> = None;
            let mut request = client.get(HOME_URL);
            for (name, value) in headers() {
                request = request.header(name, value);
            }
            if let Ok(response) = request.send().await {
                let url = response.url().to_string();
                if let Ok(html) = response.text().await
                    && !is_challenge(&url, &html)
                {
                    fields = parse_form_inputs(&html);
                }
            }

            // With the token of the form the query is posted back; without it
            // the search is a plain GET.
            let has_token = fields.is_some();
            let mut form = fields.unwrap_or_default();
            form.push(("query".into(), query.clone()));
            if let Some(recency) = params.recency {
                form.push(("with_date".into(), recency_to_with_date(recency).into()));
            }

            let mut request = if has_token {
                client.post(SEARCH_URL)
            } else {
                client.get(SEARCH_URL)
            };
            for (name, value) in headers() {
                request = request.header(name, value);
            }
            request = request.header("Referer", HOME_URL);
            if has_token {
                request = request
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(super::encode_form(&form));
            } else {
                request = request.query(&[("query", &query)]);
                if let Some(recency) = params.recency {
                    request = request.query(&[("with_date", recency_to_with_date(recency))]);
                }
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("startpage", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let url = response.url().to_string();
            let html = response.text().await.map_err(|error| {
                SearchProviderError::new(
                    "startpage",
                    format!("the answer could not be read: {error}"),
                )
            })?;

            if is_challenge(&url, &html) {
                return Err(SearchProviderError::with_status(
                    "startpage",
                    "Startpage blocked the request with its bot challenge (a proof of work). It \
                     throttles the scripted searches of a shared address: try DuckDuckGo or \
                     configure a provider with a key, such as Brave, Tavily, Exa or Kagi.",
                    429,
                ));
            }
            if !status.is_success() {
                if let Some(error) = classify_http_error("startpage", status.as_u16(), &html) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "startpage",
                    format!("Startpage answered {status}"),
                    status.as_u16(),
                ));
            }

            let mut sources: Vec<SearchSource> = Vec::new();
            let mut seen: Vec<String> = Vec::new();
            for result in parse_results(&html) {
                if seen.contains(&result.url) {
                    continue;
                }
                seen.push(result.url.clone());
                sources.push(result);
                if sources.len() >= limit {
                    break;
                }
            }
            Ok(SearchResponse {
                provider: "startpage".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
<div class="result">
  <a class="result-link" href="https://rust-lang.org/"><h2 class="wgl-title">Rust</h2></a>
  <p class="description">Rust is a fast language.</p>
</div>
<div class="result">
  <a class="result-link" href="https://www.startpage.com/privacy">Privacy</a>
  <p class="description">Not a result.</p>
</div>
"#;

    #[test]
    fn the_results_are_taken_out_of_the_page() {
        let results = parse_results(PAGE);
        assert_eq!(results.len(), 1, "{results:?}");
        assert_eq!(results[0].title, "Rust");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("Rust is a fast language.")
        );
    }

    #[test]
    fn the_links_to_startpage_itself_are_left_out() {
        assert!(sanitize_result_url("https://www.startpage.com/privacy").is_none());
        assert_eq!(
            sanitize_result_url("https://rust-lang.org/").as_deref(),
            Some("https://rust-lang.org/")
        );
    }

    #[test]
    fn the_challenge_is_recognised() {
        assert!(is_challenge("https://www.startpage.com/en/errors/", ""));
        assert!(is_challenge("", "component---src-pages-captcha"));
        assert!(is_challenge(
            "",
            "<script id=\"anubis_challenge\"></script>"
        ));
        assert!(!is_challenge(
            "https://www.startpage.com/sp/search",
            "<html>"
        ));
    }
}

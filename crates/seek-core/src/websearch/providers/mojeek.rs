//! Mojeek: an index of its own, and no key (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/mojeek.ts`.

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::html::{elements_with_class, text_of};
use crate::websearch::query::{QuerySyntax, format_scraper_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The homepage of Mojeek.
const HOME_URL: &str = "https://www.mojeek.de/?arc=none&lang=en&lb=en&theme=dark";
/// The address of the search.
const SEARCH_URL: &str = "https://www.mojeek.de/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The syntax that is sent to Mojeek.
///
/// Mojeek reads `site:`, the quoted phrases and the `-` exclusions. The `in*`
/// operators and its own date syntax are left out: the pipeline enforces them
/// on the results instead.
const MOJEEK_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
    phrases: true,
    negation: true,
    or: false,
    site: true,
    in_url: false,
    in_title: false,
    in_text: false,
    filetype: false,
    date_range: false,
};

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
        ("Referer", HOME_URL),
    ]
}

/// The address of a result, when it points away from Mojeek.
fn normalize_result_url(href: &str) -> Option<String> {
    let href = href.trim();
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
    for domain in ["mojeek.com", "mojeek.co.uk", "mojeek.fr", "mojeek.de"] {
        if host == domain || host.ends_with(&format!(".{domain}")) {
            return None;
        }
    }
    Some(absolute)
}

/// The results of a page of Mojeek, in the order of the page.
fn parse_results(html: &str) -> Vec<SearchSource> {
    let mut results: Vec<SearchSource> = Vec::new();
    let Some(list) = elements_with_class(html, "ul", "results-standard")
        .into_iter()
        .next()
    else {
        return results;
    };
    for item in elements_with_class(&list.inner, "li", "") {
        let anchor = elements_with_class(&item.inner, "a", "title")
            .into_iter()
            .next();
        let Some(anchor) = anchor else { continue };
        let Some(url) = anchor.attribute("href").and_then(normalize_result_url) else {
            continue;
        };
        let title = text_of(&anchor.inner);
        if title.is_empty() {
            continue;
        }
        let snippet = elements_with_class(&item.inner, "p", "s")
            .into_iter()
            .next()
            .map(|element| text_of(&element.inner))
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

/// Whether the page is the wall of the bot defence instead of the results.
fn is_robot_page(html: &str) -> bool {
    let robot = html.contains("altcha-widget")
        || html.contains("captcha-wrap")
        || html.to_lowercase().contains("sending automated queries");
    robot && !html.contains("results-standard")
}

/// The Mojeek search provider.
pub struct Mojeek;

impl SearchProvider for Mojeek {
    fn id(&self) -> &'static str {
        "mojeek"
    }

    fn label(&self) -> &'static str {
        "Mojeek"
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
            let query =
                format_scraper_query(&params.query, Some(&params.parsed), &MOJEEK_QUERY_SYNTAX);

            let client = super::client();

            let mut request = client.get(SEARCH_URL).query(&[
                ("q", query.as_str()),
                ("t", &limit.to_string()),
                ("arc", "none"),
                ("lang", "en"),
                ("lb", "en"),
                ("theme", "dark"),
            ]);
            // Mojeek's `since` filter takes the same words as the recency of the
            // search.
            if let Some(recency) = params.recency {
                request = request.query(&[("since", recency.as_str())]);
            }
            for (name, value) in headers() {
                request = request.header(name, value);
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("mojeek", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let html = response.text().await.map_err(|error| {
                SearchProviderError::new("mojeek", format!("the answer could not be read: {error}"))
            })?;

            if is_robot_page(&html) {
                return Err(SearchProviderError::with_status(
                    "mojeek",
                    "Mojeek blocked the request with its wall for automated queries. It \
                     throttles the scripted searches of a shared address: configure a provider \
                     with a key, such as Brave, Tavily, Exa or Kagi.",
                    429,
                ));
            }
            if !status.is_success() {
                if let Some(error) = classify_http_error("mojeek", status.as_u16(), &html) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "mojeek",
                    format!("Mojeek answered {status}"),
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
                provider: "mojeek".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
<ul class="results-standard">
  <li>
    <h2><a class="title" href="https://rust-lang.org/">Rust Programming Language</a></h2>
    <p class="s">Rust is a fast and reliable language.</p>
  </li>
  <li>
    <h2><a class="title" href="https://www.mojeek.de/about">About Mojeek</a></h2>
    <p class="s">Not a result.</p>
  </li>
</ul>
"#;

    #[test]
    fn the_results_are_taken_out_of_the_page() {
        let results = parse_results(PAGE);
        assert_eq!(results.len(), 1, "{results:?}");
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("Rust is a fast and reliable language.")
        );
    }

    #[test]
    fn the_wall_of_the_bot_defence_is_recognised() {
        assert!(is_robot_page(
            "<html><div class=\"altcha-widget\"></div></html>"
        ));
        assert!(!is_robot_page(PAGE));
    }
}

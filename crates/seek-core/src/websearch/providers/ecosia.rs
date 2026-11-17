//! Ecosia: the server-rendered results page, and no key (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/ecosia.ts`.
//!
//! The page is a Vue/Nuxt document rather than a JSON island, so the results
//! are read out of the markup: every result is an
//! `<article data-test-id="organic-result">` whose title anchor carries the
//! final address directly. Ecosia fronts the search with Cloudflare, and the
//! challenge page is recognised by its markers instead of its results.
//!
//! The recency is ignored on purpose: the page has no date filter, and the
//! legacy `freshness` parameter is a no-op on the server.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::html::{elements_with_attribute, elements_with_class, text_of};
use crate::websearch::query::{GOOGLE_QUERY_SYNTAX, format_scraper_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The homepage of Ecosia, and the referer of the search.
const HOME_URL: &str = "https://www.ecosia.org/";
/// The address of the search.
const SEARCH_URL: &str = "https://www.ecosia.org/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

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

/// The address of a result, when it points away from Ecosia.
///
/// The organic anchors carry the target directly; anything that is not
/// http(s) or that points back at Ecosia (its images and news verticals) is
/// not a result.
fn resolve_result_url(href: &str) -> Option<String> {
    let base = reqwest::Url::parse(HOME_URL).ok()?;
    let url = base.join(href.trim()).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_lowercase();
    if host == "ecosia.org" || host.ends_with(".ecosia.org") {
        return None;
    }
    Some(url.to_string())
}

/// The title of a result: the element marked `result-title`, or the anchor.
fn title_of(anchor: &str) -> Option<String> {
    for tag in ["h1", "h2", "h3", "h4", "div", "span"] {
        for element in elements_with_attribute(anchor, tag, "data-test-id", "result-title") {
            let title = text_of(&element.inner);
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    let title = text_of(anchor);
    if title.is_empty() { None } else { Some(title) }
}

/// The preview of a result: the inner description first, its container second.
fn snippet_of(article: &str) -> Option<String> {
    for test_id in ["web-result-description", "result-description"] {
        for tag in ["p", "div", "span"] {
            for element in elements_with_attribute(article, tag, "data-test-id", test_id) {
                let text = text_of(&element.inner);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// The results of a page of Ecosia, in the order of the page.
fn parse_results(html: &str) -> Vec<SearchSource> {
    let mut results: Vec<SearchSource> = Vec::new();
    for article in elements_with_attribute(html, "article", "data-test-id", "organic-result") {
        let Some(anchor) = elements_with_class(&article.inner, "a", "")
            .into_iter()
            .find(|anchor| anchor.inner.contains("result-title"))
        else {
            continue;
        };
        let Some(url) = anchor.attribute("href").and_then(resolve_result_url) else {
            continue;
        };
        let Some(title) = title_of(&anchor.inner) else {
            continue;
        };
        results.push(SearchSource {
            title,
            url,
            snippet: snippet_of(&article.inner),
            ..Default::default()
        });
    }
    results
}

/// Whether the page is the Cloudflare challenge instead of the results.
///
/// The observed page is a 403 titled "Ecosia Firewall" carrying the
/// `_cf_chl_opt` bootstrap and the challenge-platform loader.
fn is_blocked_page(html: &str) -> bool {
    let lower = html.to_lowercase();
    lower.contains("ecosia firewall")
        || lower.contains("_cf_chl_opt")
        || lower.contains("/cdn-cgi/challenge-platform/")
        || (lower.contains("confirm you") && lower.contains("not a robot"))
}

/// The Ecosia search provider.
pub struct Ecosia;

impl SearchProvider for Ecosia {
    fn id(&self) -> &'static str {
        "ecosia"
    }

    fn label(&self) -> &'static str {
        "Ecosia"
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
            // Ecosia serves Google-backed results, so the classic operators pass
            // inline; the scraper-hostile ones are demoted first.
            let query =
                format_scraper_query(&params.query, Some(&params.parsed), &GOOGLE_QUERY_SYNTAX);

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("ecosia", format!("the client failed: {error}"))
                })?;

            let mut request = client.get(SEARCH_URL).query(&[("q", query.as_str())]);
            for (name, value) in headers() {
                request = request.header(name, value);
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("ecosia", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let html = response.text().await.map_err(|error| {
                SearchProviderError::new("ecosia", format!("the answer could not be read: {error}"))
            })?;

            if is_blocked_page(&html) {
                return Err(SearchProviderError::with_status(
                    "ecosia",
                    "Ecosia blocked the request with a Cloudflare bot challenge. Ecosia's \
                     firewall throttles automated searches from shared addresses: configure a \
                     provider with a key, such as Brave, Tavily, Exa or Kagi.",
                    429,
                ));
            }
            if !status.is_success() {
                if let Some(error) = classify_http_error("ecosia", status.as_u16(), &html) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "ecosia",
                    format!("Ecosia answered {status}"),
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
                provider: "ecosia".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
<article data-test-id="organic-result">
  <a href="https://rust-lang.org/">
    <h2 data-test-id="result-title">Rust Programming Language</h2>
  </a>
  <p data-test-id="web-result-description">Rust is a fast and reliable language.</p>
</article>
<article data-test-id="organic-result">
  <a href="https://www.ecosia.org/images?q=rust">
    <h2 data-test-id="result-title">Images of Rust</h2>
  </a>
</article>
<article data-test-id="organic-result">
  <a href="https://doc.rust-lang.org/book/">
    <h2 data-test-id="result-title">The Rust book</h2>
  </a>
  <div data-test-id="result-description">Learn Rust here.</div>
</article>
"#;

    #[test]
    fn the_results_are_taken_out_of_the_page() {
        let results = parse_results(PAGE);
        assert_eq!(results.len(), 2, "{results:?}");
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("Rust is a fast and reliable language.")
        );
        assert_eq!(results[1].title, "The Rust book");
        assert_eq!(results[1].snippet.as_deref(), Some("Learn Rust here."));
    }

    #[test]
    fn the_challenge_page_is_recognised() {
        assert!(is_blocked_page(
            "<html><title>Ecosia Firewall</title></html>"
        ));
        assert!(is_blocked_page("<script>var _cf_chl_opt={};</script>"));
        assert!(!is_blocked_page(PAGE));
    }

    #[test]
    fn a_relative_address_is_resolved() {
        assert_eq!(
            resolve_result_url("/search?q=rust").as_deref(),
            None,
            "an address back at Ecosia is not a result"
        );
        assert_eq!(
            resolve_result_url("https://rust-lang.org/").as_deref(),
            Some("https://rust-lang.org/")
        );
    }
}

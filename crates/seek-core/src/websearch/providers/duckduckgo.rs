//! DuckDuckGo: the HTML search that needs no key (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/duckduckgo.ts`.
//!
//! The no-JS HTML frontend of DuckDuckGo answers a POST with a static results
//! page that can be read without a browser. DuckDuckGo answers a plain GET
//! with a bot challenge, so the request carries the headers of a browser.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::html::{elements_with_class, text_of};
use crate::websearch::query::{QuerySyntax, format_scraper_query};
use crate::websearch::types::{SearchProviderError, SearchResponse, SearchSource};

/// The search page of DuckDuckGo.
const SEARCH_URL: &str = "https://html.duckduckgo.com/html/";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The syntax DuckDuckGo parses. The date bounds are left out on purpose:
/// DuckDuckGo does not parse them, so they are enforced by the filter of the
/// pipeline instead.
const DDG_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
    phrases: true,
    negation: true,
    or: true,
    site: true,
    in_url: true,
    in_title: true,
    in_text: true,
    filetype: true,
    date_range: false,
};

/// The recency of a search, as the `df` field of the form.
fn recency_to_df(recency: super::super::types::Recency) -> &'static str {
    match recency {
        super::super::types::Recency::Day => "d",
        super::super::types::Recency::Week => "w",
        super::super::types::Recency::Month => "m",
        super::super::types::Recency::Year => "y",
    }
}

/// The region codes of DuckDuckGo.
///
/// The codes look like `region-language` locales, but they carry identifiers
/// that cannot be derived (`jp-jp`, `tw-tzh`, `uk-en`).
const KL_CODES: [&str; 68] = [
    "xa-ar", "xa-en", "ar-es", "au-en", "at-de", "be-fr", "be-nl", "br-pt", "bg-bg", "ca-en",
    "ca-fr", "ct-ca", "cl-es", "cn-zh", "co-es", "hr-hr", "cz-cs", "dk-da", "ee-et", "fi-fi",
    "fr-fr", "de-de", "gr-el", "hk-tzh", "hu-hu", "in-en", "id-id", "id-en", "ie-en", "il-he",
    "it-it", "jp-jp", "kr-kr", "lv-lv", "lt-lt", "xl-es", "my-ms", "my-en", "mx-es", "nl-nl",
    "nz-en", "no-no", "pe-es", "ph-en", "ph-tl", "pl-pl", "pt-pt", "ro-ro", "ru-ru", "sg-en",
    "sk-sk", "sl-sl", "za-en", "es-es", "se-sv", "ch-de", "ch-fr", "ch-it", "tw-tzh", "th-th",
    "tr-tr", "ua-uk", "uk-en", "us-en", "ue-es", "ve-es", "vn-vi", "wt-wt",
];

/// The locales whose code is not a swap of the parts.
const LOCALE_ALIASES: [(&str, &str); 7] = [
    ("ca-es", "ct-ca"),
    ("en-gb", "uk-en"),
    ("es-419", "xl-es"),
    ("es-us", "ue-es"),
    ("ja-jp", "jp-jp"),
    ("ko-kr", "kr-kr"),
    ("zh-tw", "tw-tzh"),
];

/// The code of DuckDuckGo for a language of the query.
pub fn locale_to_kl(language: Option<&str>) -> Option<&'static str> {
    let language = language?;
    let locale = language.to_lowercase().replace('_', "-");
    if let Some((_, alias)) = LOCALE_ALIASES.iter().find(|(key, _)| *key == locale) {
        return Some(alias);
    }
    if locale == "zh-hk" {
        return Some("hk-tzh");
    }
    let (language, region) = locale.split_once('-')?;
    if language.len() != 2 || region.len() != 2 {
        return None;
    }
    let candidate = format!("{region}-{language}");
    KL_CODES.iter().find(|code| **code == candidate).copied()
}

/// The headers of a browser. DuckDuckGo answers a request without them with a
/// challenge.
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
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Referer", "https://html.duckduckgo.com/"),
    ]
}

/// Resolve the address of a result back to the page it points at.
///
/// DuckDuckGo routes the clicks through `//duckduckgo.com/l/?uddg=<encoded>` so
/// it can count them; the address that is wanted is the one inside.
fn unwrap_result_url(href: &str) -> Option<String> {
    if href.is_empty() {
        return None;
    }
    let decoded = href.replace("&amp;", "&");
    // The advertisements go elsewhere.
    if decoded.contains("duckduckgo.com/y.js") {
        return None;
    }
    if let Some(at) = decoded.find("uddg=") {
        let rest = &decoded[at + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        return percent_decode(&rest[..end]);
    }
    if decoded.starts_with("//") {
        return Some(format!("https:{decoded}"));
    }
    if decoded.starts_with("http://") || decoded.starts_with("https://") {
        return Some(decoded);
    }
    None
}

/// Decode the `%xx` escapes of a text.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).ok()
}

/// The publication time of a result, when the row carries one.
fn extract_published_date(block: &str) -> Option<String> {
    let extras = elements_with_class(block, "div", "result__extras__url")
        .into_iter()
        .next()?;
    for span in elements_with_class(&extras.inner, "span", "") {
        let text = text_of(&span.inner);
        if text.len() >= 10
            && text.as_bytes().get(4).is_some_and(|byte| *byte == b'-')
            && text.as_bytes().get(7).is_some_and(|byte| *byte == b'-')
            && text[..4]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            return Some(text);
        }
    }
    None
}

/// The results of a page of DuckDuckGo, in the order of the page.
fn parse_results(html: &str) -> Vec<SearchSource> {
    let mut results: Vec<SearchSource> = Vec::new();
    for block in elements_with_class(html, "div", "result") {
        let Some(anchor) = elements_with_class(&block.inner, "a", "result__a")
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(url) = anchor
            .attribute("href")
            .and_then(unwrap_result_url)
            .filter(|url| !url.is_empty())
        else {
            continue;
        };
        let title = text_of(&anchor.inner);
        if title.is_empty() {
            continue;
        }
        let snippet = elements_with_class(&block.inner, "a", "result__snippet")
            .into_iter()
            .next()
            .map(|element| text_of(&element.inner))
            .filter(|text| !text.is_empty());
        let published_date = extract_published_date(&block.inner);
        let age_seconds = published_date
            .as_deref()
            .and_then(super::super::types::date_to_age_seconds);
        results.push(SearchSource {
            title,
            url,
            snippet,
            published_date,
            age_seconds,
        });
    }
    results
}

/// The hidden fields of the form of the next page.
fn parse_continuation(html: &str) -> Option<Vec<(String, String)>> {
    for form in elements_with_class(html, "form", "") {
        let inputs = crate::websearch::html::hidden_inputs(&form.inner);
        let names: Vec<&str> = inputs.iter().map(|(name, _)| name.as_str()).collect();
        if names.contains(&"s") && names.contains(&"vqd") {
            return Some(inputs);
        }
    }
    None
}

/// Whether the page is the bot challenge instead of the results.
fn is_anomaly(html: &str) -> bool {
    html.contains("anomaly-modal") || html.contains("anomaly.js")
}

/// The DuckDuckGo search provider.
pub struct DuckDuckGo;

impl SearchProvider for DuckDuckGo {
    fn id(&self) -> &'static str {
        "duckduckgo"
    }

    fn label(&self) -> &'static str {
        "DuckDuckGo"
    }

    fn is_available(&self, _params: &SearchParams) -> bool {
        true
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let limit =
                super::super::types::clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let query =
                format_scraper_query(&params.query, Some(&params.parsed), &DDG_QUERY_SYNTAX);
            let kl = locale_to_kl(params.parsed.lang.as_deref()).unwrap_or("us-en");

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("duckduckgo", format!("the client failed: {error}"))
                })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            let mut seen: Vec<String> = Vec::new();
            let mut fields: Vec<(String, String)> = vec![
                ("q".into(), query),
                ("kl".into(), kl.into()),
                ("b".into(), String::new()),
            ];
            if let Some(recency) = params.recency {
                fields.push(("df".into(), recency_to_df(recency).into()));
            }

            // DuckDuckGo answers ten results a page: the form of the next page
            // is asked for when there are not enough.
            for _ in 0..5 {
                let body = super::encode_form(&fields);
                let mut request = client.post(SEARCH_URL);
                for (name, value) in headers() {
                    request = request.header(name, value);
                }
                let response = request.body(body).send().await.map_err(|error| {
                    SearchProviderError::new("duckduckgo", format!("the search failed: {error}"))
                })?;
                let status = response.status();
                let html = response.text().await.map_err(|error| {
                    SearchProviderError::new(
                        "duckduckgo",
                        format!("the answer could not be read: {error}"),
                    )
                })?;
                if !status.is_success() {
                    if let Some(error) = classify_http_error("duckduckgo", status.as_u16(), &html) {
                        return Err(error);
                    }
                    return Err(SearchProviderError::with_status(
                        "duckduckgo",
                        format!("DuckDuckGo answered {status}"),
                        status.as_u16(),
                    ));
                }
                if is_anomaly(&html) {
                    return Err(SearchProviderError::with_status(
                        "duckduckgo",
                        "DuckDuckGo blocked the request with a bot challenge. It throttles the \
                         scripted searches of a shared address: configure a provider with a key, \
                         such as Brave, Tavily, Exa or Kagi.",
                        429,
                    ));
                }

                let before = sources.len();
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
                if sources.len() >= limit || sources.len() == before {
                    break;
                }
                let Some(inputs) = parse_continuation(&html) else {
                    break;
                };
                fields = inputs;
                fields.push((
                    "q".into(),
                    super::super::query::format_scraper_query(
                        &params.query,
                        Some(&params.parsed),
                        &DDG_QUERY_SYNTAX,
                    ),
                ));
                fields.push(("kl".into(), kl.into()));
                fields.push(("b".into(), String::new()));
            }

            Ok(SearchResponse {
                provider: "duckduckgo".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
<div class="result results_links results_links_deep web-result">
  <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org%2F&amp;rut=1"><b>Rust</b> Programming Language</a>
  <a class="result__snippet" href="https://rust-lang.org/"><b>Rust</b> is a fast and reliable language.</a>
</div>
<div class="result results_links">
  <a class="result__a" href="https://doc.rust-lang.org/book/">The Rust book</a>
  <a class="result__snippet" href="https://doc.rust-lang.org/book/">Learn Rust here.</a>
</div>
"#;

    #[test]
    fn the_results_are_taken_out_of_the_page() {
        let results = parse_results(PAGE);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("Rust is a fast and reliable language.")
        );
        assert_eq!(results[1].title, "The Rust book");
    }

    #[test]
    fn the_redirect_of_a_click_is_unwrapped() {
        assert_eq!(
            unwrap_result_url("//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org%2F").as_deref(),
            Some("https://rust-lang.org/")
        );
        assert_eq!(
            unwrap_result_url("https://doc.rust-lang.org/").as_deref(),
            Some("https://doc.rust-lang.org/")
        );
    }

    #[test]
    fn the_advertisements_are_left_out() {
        assert!(unwrap_result_url("//duckduckgo.com/y.js?ad=1").is_none());
    }

    #[test]
    fn the_language_is_mapped_to_a_region() {
        assert_eq!(locale_to_kl(Some("en-us")), Some("us-en"));
        assert_eq!(locale_to_kl(Some("de-de")), Some("de-de"));
        assert_eq!(locale_to_kl(Some("ja-jp")), Some("jp-jp"));
        assert_eq!(locale_to_kl(Some("xx-yy")), None);
    }
}

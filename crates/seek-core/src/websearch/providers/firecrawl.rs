//! Firecrawl: the search API, with a key or without one (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/firecrawl.ts`.
//!
//! Firecrawl is SERP-backed, so the classic Google operators pass through, and
//! the absolute date bounds of the query move into its `tbs` parameter. The
//! key is in `FIRECRAWL_API_KEY`, or in the auth file under that name; without
//! a key the API is asked in keyless mode. `FIRECRAWL_BASE_URL` (or
//! `FIRECRAWL_API_URL`) points the provider at a self-hosted instance.

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{GOOGLE_QUERY_SYNTAX, QuerySyntax, format_query};
use crate::websearch::types::{
    Recency, SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The address of the API when the user hosts none.
const DEFAULT_BASE_URL: &str = "https://api.firecrawl.dev/v2";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 100;

/// The syntax of the query when the date bounds go into `tbs`.
const FIRECRAWL_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
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

/// The window of a recency, as the `qdr` of `tbs`.
fn recency_to_tbs(recency: Recency) -> &'static str {
    match recency {
        Recency::Day => "qdr:d",
        Recency::Week => "qdr:w",
        Recency::Month => "qdr:m",
        Recency::Year => "qdr:y",
    }
}

/// An ISO `YYYY-MM-DD` as the `MM/DD/YYYY` of a Google `tbs` range.
fn to_google_date(iso: &str) -> String {
    let mut parts = iso.split('-');
    let year = parts.next().unwrap_or_default();
    let month = parts.next().unwrap_or_default();
    let day = parts.next().unwrap_or_default();
    format!("{month}/{day}/{year}")
}

/// The `tbs` of the absolute bounds of the query, when it has any.
fn date_tbs(params: &SearchParams) -> Option<String> {
    if params.parsed.after.is_none() && params.parsed.before.is_none() {
        return None;
    }
    let mut parts = vec!["cdr:1".to_string()];
    if let Some(after) = params.parsed.after.as_deref() {
        parts.push(format!("cd_min:{}", to_google_date(after)));
    }
    if let Some(before) = params.parsed.before.as_deref() {
        parts.push(format!("cd_max:{}", to_google_date(before)));
    }
    Some(parts.join(","))
}

/// The address of the search, honouring the self-hosting override.
fn search_url() -> Result<String, SearchProviderError> {
    let configured = std::env::var("FIRECRAWL_BASE_URL")
        .or_else(|_| std::env::var("FIRECRAWL_API_URL"))
        .ok()
        .filter(|value| !value.trim().is_empty());
    let Some(configured) = configured else {
        return Ok(format!("{DEFAULT_BASE_URL}/search"));
    };
    let url = reqwest::Url::parse(configured.trim()).map_err(|_| {
        SearchProviderError::new(
            "firecrawl",
            "the Firecrawl base URL is not an HTTP address (FIRECRAWL_BASE_URL)",
        )
    })?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(SearchProviderError::new(
            "firecrawl",
            "the Firecrawl base URL has to be http or https (FIRECRAWL_BASE_URL)",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SearchProviderError::new(
            "firecrawl",
            "the Firecrawl base URL cannot carry credentials (FIRECRAWL_BASE_URL)",
        ));
    }
    let mut base = url.as_str().trim_end_matches('/').to_string();
    let path = url.path().trim_end_matches('/');
    if !(path.ends_with("/v1") || path.ends_with("/v2")) {
        base.push_str("/v2");
    }
    Ok(format!("{base}/search"))
}

/// The results of a response: a flat array, or the web half of the buckets.
fn web_results(data: &serde_json::Value) -> &[serde_json::Value] {
    let empty: &[serde_json::Value] = &[];
    match data.get("data") {
        Some(serde_json::Value::Array(results)) => results,
        Some(serde_json::Value::Object(buckets)) => buckets
            .get("web")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(empty),
        _ => data
            .get("results")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(empty),
    }
}

/// The text of a field of a result.
fn text_of(value: Option<&serde_json::Value>) -> Option<String> {
    let text = value.and_then(serde_json::Value::as_str)?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.to_string())
}

/// The Firecrawl search provider.
pub struct Firecrawl;

impl SearchProvider for Firecrawl {
    fn id(&self) -> &'static str {
        "firecrawl"
    }

    fn label(&self) -> &'static str {
        "Firecrawl"
    }

    /// Always available: without a key the API answers in keyless mode.
    fn is_available(&self, _params: &SearchParams) -> bool {
        true
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let mut query = params.query.clone();
            let mut tbs = date_tbs(params);
            if params.parsed.has_directives {
                // The absolute date bounds move into `tbs` and are stripped from
                // the query string.
                let syntax = if tbs.is_some() {
                    FIRECRAWL_QUERY_SYNTAX
                } else {
                    GOOGLE_QUERY_SYNTAX
                };
                query = format_query(&params.parsed, &syntax);
            }
            if tbs.is_none() {
                tbs = params
                    .recency
                    .map(|recency| recency_to_tbs(recency).to_string());
            }

            let mut body = serde_json::json!({
                "query": query,
                "limit": limit,
                "sources": [{"type": "web"}],
            });
            if let Some(tbs) = &tbs {
                body["tbs"] = serde_json::Value::String(tbs.clone());
            }

            let client = super::client();

            let mut request = client
                .post(search_url()?)
                .header("Content-Type", "application/json")
                .json(&body);
            if let Some(key) = params.key("firecrawl") {
                request = request.header("Authorization", format!("Bearer {key}"));
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("firecrawl", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new(
                    "firecrawl",
                    format!("the answer could not be read: {error}"),
                )
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("firecrawl", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "firecrawl",
                    format!("Firecrawl answered {status}: {text}"),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "firecrawl",
                    format!("Firecrawl answered with invalid JSON: {error}"),
                    500,
                )
            })?;
            if data.get("success").and_then(serde_json::Value::as_bool) == Some(false) {
                let message =
                    text_of(data.get("error")).unwrap_or_else(|| "the request failed".to_string());
                return Err(SearchProviderError::new("firecrawl", message));
            }

            let mut sources: Vec<SearchSource> = Vec::new();
            for result in web_results(&data) {
                let Some(url) = text_of(result.get("url")) else {
                    continue;
                };
                sources.push(SearchSource {
                    title: text_of(result.get("title")).unwrap_or_else(|| url.clone()),
                    url,
                    snippet: text_of(result.get("description"))
                        .or_else(|| text_of(result.get("snippet")))
                        .or_else(|| text_of(result.get("markdown"))),
                    ..Default::default()
                });
                if sources.len() >= limit {
                    break;
                }
            }
            Ok(SearchResponse {
                provider: "firecrawl".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_bounds_become_a_google_range() {
        assert_eq!(to_google_date("2024-03-05"), "03/05/2024");
    }

    #[test]
    fn the_recency_is_a_tbs_window() {
        assert_eq!(recency_to_tbs(Recency::Week), "qdr:w");
        assert_eq!(recency_to_tbs(Recency::Year), "qdr:y");
    }

    #[test]
    fn the_web_results_are_read_from_every_shape() {
        let flat = serde_json::json!({"data": [{"url": "https://a/"}]});
        assert_eq!(web_results(&flat).len(), 1);
        let bucketed = serde_json::json!({"data": {"web": [{"url": "https://a/"}]}});
        assert_eq!(web_results(&bucketed).len(), 1);
        let legacy = serde_json::json!({"results": [{"url": "https://a/"}]});
        assert_eq!(web_results(&legacy).len(), 1);
    }
}

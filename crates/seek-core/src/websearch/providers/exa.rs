//! Exa: the search API of Exa (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/exa.ts`.
//!
//! The key is in `EXA_API_KEY`, or in the auth file under that name.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results, date_to_age_seconds,
};

/// The address of the search.
const SEARCH_URL: &str = "https://api.exa.ai/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;
/// The most characters of a preview.
const MAX_SNIPPET_CHARS: usize = 500;

/// The syntax that is sent to Exa: the quoted phrases. The other directives go
/// into the parameters of the API.
const EXA_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
    phrases: true,
    negation: false,
    or: false,
    site: false,
    in_url: false,
    in_title: false,
    in_text: false,
    filetype: false,
    date_range: false,
};

/// The hosts of the `site:` values, without the paths and without a duplicate.
fn site_hosts(sites: &[String]) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for site in sites {
        let host = site.split('/').next().unwrap_or(site).to_string();
        if !host.is_empty() && !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    hosts
}

/// The preview of a result: its summary, its text, or its highlights.
fn build_snippet(result: &serde_json::Value) -> Option<String> {
    let mut text = result
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            result
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    if text.is_none()
        && let Some(highlights) = result
            .get("highlights")
            .and_then(serde_json::Value::as_array)
    {
        let parts: Vec<&str> = highlights
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        if !parts.is_empty() {
            text = Some(parts.join(" "));
        }
    }
    let text = text?;
    if text.chars().count() > MAX_SNIPPET_CHARS {
        let mut out: String = text.chars().take(MAX_SNIPPET_CHARS).collect();
        out.push('…');
        return Some(out);
    }
    Some(text)
}

/// The Exa search provider.
pub struct Exa;

impl SearchProvider for Exa {
    fn id(&self) -> &'static str {
        "exa"
    }

    fn label(&self) -> &'static str {
        "Exa"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params.key("exa").is_some()
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(key) = params.key("exa") else {
                return Err(SearchProviderError::new(
                    "exa",
                    "Exa needs a key: set EXA_API_KEY, or put it in state/auth.json",
                ));
            };
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let query = if params.parsed.has_directives {
                format_query(&params.parsed, &EXA_QUERY_SYNTAX)
            } else {
                params.query.trim().to_string()
            };
            if query.is_empty() {
                return Err(SearchProviderError::with_status(
                    "exa",
                    "Exa needs search terms: a query of dates only is not supported",
                    400,
                ));
            }

            let mut body = serde_json::json!({
                "query": query,
                "numResults": limit,
                "type": "auto",
                "contents": { "summary": { "query": query } },
            });
            let include = site_hosts(&params.parsed.sites);
            if !include.is_empty() {
                body["includeDomains"] = serde_json::json!(include);
            }
            let exclude = site_hosts(&params.parsed.excluded_sites);
            if !exclude.is_empty() {
                body["excludeDomains"] = serde_json::json!(exclude);
            }
            if let Some(after) = &params.parsed.after {
                body["startPublishedDate"] = serde_json::json!(after);
            }
            if let Some(before) = &params.parsed.before {
                body["endPublishedDate"] = serde_json::json!(before);
            }

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("exa", format!("the client failed: {error}"))
                })?;

            let response = client
                .post(SEARCH_URL)
                .header("Content-Type", "application/json")
                .header("x-api-key", key)
                .json(&body)
                .send()
                .await
                .map_err(|error| {
                    SearchProviderError::new("exa", format!("the search failed: {error}"))
                })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new("exa", format!("the answer could not be read: {error}"))
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("exa", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "exa",
                    format!("Exa answered {status}: {}", text.trim()),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "exa",
                    format!("Exa answered with invalid JSON: {error}"),
                    500,
                )
            })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            if let Some(results) = data.get("results").and_then(serde_json::Value::as_array) {
                for result in results {
                    let Some(url) = result.get("url").and_then(serde_json::Value::as_str) else {
                        continue;
                    };
                    let published_date = result
                        .get("publishedDate")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    sources.push(SearchSource {
                        title: result
                            .get("title")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(url)
                            .to_string(),
                        url: url.to_string(),
                        snippet: build_snippet(result),
                        age_seconds: published_date.as_deref().and_then(date_to_age_seconds),
                        published_date,
                    });
                    if sources.len() >= limit {
                        break;
                    }
                }
            }
            Ok(SearchResponse {
                provider: "exa".into(),
                sources,
            })
        })
    }
}

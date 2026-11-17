//! Jina: the search of Jina Reader (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/jina.ts`.
//!
//! The key is in `JINA_API_KEY`, or in the auth file under that name.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The address of the search.
const SEARCH_URL: &str = "https://s.jina.ai";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 5;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The Jina search provider.
pub struct Jina;

impl SearchProvider for Jina {
    fn id(&self) -> &'static str {
        "jina"
    }

    fn label(&self) -> &'static str {
        "Jina"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params.key("jina").is_some()
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(key) = params.key("jina") else {
                return Err(SearchProviderError::new(
                    "jina",
                    "Jina needs a key: set JINA_API_KEY, or put it in state/auth.json",
                ));
            };
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);

            // One site goes into the header of the request; more than one stays
            // in the query.
            let single_site = if params.parsed.sites.len() == 1 {
                params.parsed.sites[0]
                    .split('/')
                    .next()
                    .unwrap_or(&params.parsed.sites[0])
                    .to_string()
            } else {
                String::new()
            };
            let syntax = QuerySyntax {
                phrases: true,
                negation: true,
                or: false,
                site: single_site.is_empty(),
                in_url: true,
                in_title: true,
                in_text: false,
                filetype: true,
                date_range: false,
            };
            let query = if params.parsed.has_directives {
                format_query(&params.parsed, &syntax)
            } else {
                params.query.trim().to_string()
            };
            if query.is_empty() {
                return Err(SearchProviderError::with_status(
                    "jina",
                    "Jina needs search terms: a query of dates only is not supported",
                    400,
                ));
            }

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("jina", format!("the client failed: {error}"))
                })?;

            let mut request = client
                .get(format!("{SEARCH_URL}/{}", super::encode(&query)))
                .query(&[("count", &limit.to_string())])
                .header("Accept", "application/json")
                .header("Authorization", format!("Bearer {key}"))
                .header("X-Respond-With", "no-content")
                .header("X-Retain-Images", "none");
            if !single_site.is_empty() {
                request = request.header("X-Site", single_site);
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("jina", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new("jina", format!("the answer could not be read: {error}"))
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("jina", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "jina",
                    format!("Jina answered {status}: {}", text.trim()),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "jina",
                    format!("Jina answered with invalid JSON: {error}"),
                    500,
                )
            })?;

            if let Some(code) = data.get("code").and_then(serde_json::Value::as_u64)
                && code != 200
            {
                return Err(SearchProviderError::with_status(
                    "jina",
                    format!("Jina reported the failure {code}"),
                    code.min(u16::MAX as u64) as u16,
                ));
            }
            let results = data
                .get("data")
                .and_then(serde_json::Value::as_array)
                .or_else(|| data.as_array())
                .ok_or_else(|| {
                    SearchProviderError::with_status(
                        "jina",
                        "Jina answered with a shape that was not expected",
                        502,
                    )
                })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            for result in results {
                let Some(url) = result.get("url").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let snippet = result
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .or_else(|| {
                        result
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                    })
                    .map(str::to_string);
                sources.push(SearchSource {
                    title: result
                        .get("title")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(url)
                        .to_string(),
                    url: url.to_string(),
                    snippet,
                    ..Default::default()
                });
                if sources.len() >= limit {
                    break;
                }
            }
            Ok(SearchResponse {
                provider: "jina".into(),
                sources,
            })
        })
    }
}

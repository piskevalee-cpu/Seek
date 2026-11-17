//! Brave: the web search API of Brave (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/brave.ts`.
//!
//! The key is in `BRAVE_API_KEY`, or in the auth file under that name.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_query};
use crate::websearch::types::{
    Recency, SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
    date_to_age_seconds,
};

/// The address of the search.
const SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;
/// The most characters a query may have.
const MAX_QUERY_CHARACTERS: usize = 500;

/// The syntax that is sent to Brave.
///
/// Brave reads the classic operators, but not the date bounds: those go into
/// the `freshness` parameter instead.
const BRAVE_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
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

/// The freshness of a search: the period of the recency, or the bounds of the
/// dates of the query.
fn freshness(params: &SearchParams) -> Option<String> {
    if params.parsed.after.is_some() || params.parsed.before.is_some() {
        let start = params.parsed.after.as_deref().unwrap_or("1970-01-01");
        let end = params.parsed.before.as_deref().unwrap_or("2099-12-31");
        return Some(format!("{start}to{end}"));
    }
    Some(
        match params.recency? {
            Recency::Day => "pd",
            Recency::Week => "pw",
            Recency::Month => "pm",
            Recency::Year => "py",
        }
        .to_string(),
    )
}

/// The text of a field of a result.
fn normalize_text(value: Option<&str>, max: usize) -> Option<String> {
    let text = value?;
    let text = crate::websearch::html::text_of(text);
    if text.is_empty() {
        return None;
    }
    Some(if text.chars().count() > max {
        let mut out: String = text.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        text
    })
}

/// The address of a result, when it is an http address.
fn normalize_url(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.len() > 2048 {
        return None;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

/// The preview of a result: its description and the extra snippets.
fn build_snippet(result: &serde_json::Value) -> Option<String> {
    let mut snippets: Vec<String> = Vec::new();
    if let Some(description) = result
        .get("description")
        .and_then(serde_json::Value::as_str)
        && let Some(text) = normalize_text(Some(description), 8_000)
    {
        snippets.push(text);
    }
    if let Some(extras) = result
        .get("extra_snippets")
        .and_then(serde_json::Value::as_array)
    {
        for extra in extras {
            if let Some(text) = extra.as_str()
                && let Some(text) = normalize_text(Some(text), 8_000)
            {
                snippets.push(text);
            }
        }
    }
    if snippets.is_empty() {
        return None;
    }
    let combined = snippets.join("\n");
    Some(if combined.chars().count() > 8_000 {
        let mut out: String = combined.chars().take(7_999).collect();
        out.push('…');
        out
    } else {
        combined
    })
}

/// The Brave search provider.
pub struct Brave;

impl SearchProvider for Brave {
    fn id(&self) -> &'static str {
        "brave"
    }

    fn label(&self) -> &'static str {
        "Brave"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params.key("brave").is_some()
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(key) = params.key("brave") else {
                return Err(SearchProviderError::new(
                    "brave",
                    "Brave needs a key: set BRAVE_API_KEY, or put it in state/auth.json",
                ));
            };
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let query = if params.parsed.has_directives {
                format_query(&params.parsed, &BRAVE_QUERY_SYNTAX)
            } else {
                params.query.clone()
            };
            if query.chars().count() > MAX_QUERY_CHARACTERS {
                return Err(SearchProviderError::with_status(
                    "brave",
                    format!(
                        "a query of Brave cannot be longer than {MAX_QUERY_CHARACTERS} characters"
                    ),
                    400,
                ));
            }

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("brave", format!("the client failed: {error}"))
                })?;

            let mut request = client.get(SEARCH_URL).query(&[
                ("q", query.as_str()),
                ("count", &limit.to_string()),
                ("extra_snippets", "true"),
                ("text_decorations", "false"),
                ("safesearch", "moderate"),
            ]);
            if let Some(freshness) = freshness(params) {
                request = request.query(&[("freshness", &freshness)]);
            }
            let response = request
                .header("Accept", "application/json")
                .header("X-Subscription-Token", key)
                .send()
                .await
                .map_err(|error| {
                    SearchProviderError::new("brave", format!("the search failed: {error}"))
                })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new("brave", format!("the answer could not be read: {error}"))
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("brave", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "brave",
                    format!("Brave answered {status}"),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "brave",
                    format!("Brave answered with invalid JSON: {error}"),
                    500,
                )
            })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            if let Some(results) = data
                .get("web")
                .and_then(|web| web.get("results"))
                .and_then(serde_json::Value::as_array)
            {
                for result in results {
                    let Some(url) =
                        normalize_url(result.get("url").and_then(serde_json::Value::as_str))
                    else {
                        continue;
                    };
                    let published_date =
                        normalize_text(result.get("age").and_then(serde_json::Value::as_str), 100);
                    sources.push(SearchSource {
                        title: normalize_text(
                            result.get("title").and_then(serde_json::Value::as_str),
                            300,
                        )
                        .unwrap_or_else(|| url.clone()),
                        url,
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
                provider: "brave".into(),
                sources,
            })
        })
    }
}

//! Parallel: the keyless public MCP search endpoint (SPEC 9.3).
//!
//! This is the first provider of the default chain of oh-my-pi (MIT). Unlike the HTML scrapers it
//! needs no browser: a JSON-RPC `tools/call` to `https://search.parallel.ai/mcp` answers with
//! LLM-friendly results, and works from a terminal without a display. Ported from the reference
//! `web/search/providers/parallel.ts`.

use std::sync::OnceLock;
use std::time::Duration;

use super::{SearchParams, SearchProvider};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_scraper_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results, date_to_age_seconds,
};

/// The public MCP endpoint. A key is optional; without one the free tier applies.
const MCP_URL: &str = "https://search.parallel.ai/mcp";
const DEFAULT_RESULTS: usize = 10;
const MAX_RESULTS: usize = 20;

const SYNTAX: QuerySyntax = QuerySyntax {
    phrases: false,
    negation: false,
    or: false,
    site: true,
    in_url: false,
    in_title: false,
    in_text: false,
    filetype: false,
    date_range: false,
};

/// One id per process, reused on every call: the free tier uses it for rate limiting.
fn session_id() -> &'static str {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION.get_or_init(|| {
        format!(
            "seek-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        )
    })
}

/// Turn a query into the 3-6 word keyword queries Parallel expects.
fn search_queries(params: &SearchParams) -> Vec<String> {
    let query = format_scraper_query(&params.query, Some(&params.parsed), &SYNTAX);
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.len() <= 6 {
        return vec![query];
    }
    words
        .chunks(6)
        .take(3)
        .map(|chunk| chunk.join(" "))
        .collect()
}

/// The text of a JSON field.
fn field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Read the sources out of the MCP content block.
fn parse_sources(text: &str, limit: usize) -> Vec<SearchSource> {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(results) = payload.get("results").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    results
        .iter()
        .take(limit)
        .filter_map(|result| {
            let url = field(result, "url")?;
            let title = field(result, "title").unwrap_or_else(|| url.clone());
            let excerpt = result
                .get("excerpts")
                .and_then(|value| value.as_array())
                .and_then(|excerpts| excerpts.first())
                .and_then(|excerpt| excerpt.as_str())
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string);
            let published = field(result, "publish_date");
            Some(SearchSource {
                title,
                url,
                snippet: excerpt,
                age_seconds: published.as_deref().and_then(date_to_age_seconds),
                published_date: published,
            })
        })
        .collect()
}

/// The provider of the public Parallel MCP.
pub struct Parallel;

impl SearchProvider for Parallel {
    fn id(&self) -> &'static str {
        "parallel"
    }

    fn label(&self) -> &'static str {
        "Parallel"
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
            let queries = search_queries(params);
            let objective = if params.query.chars().count() > 200 {
                params.query.chars().take(200).collect()
            } else {
                params.query.clone()
            };
            let mut arguments = serde_json::json!({
                "objective": objective,
                "search_queries": queries,
                "session_id": session_id(),
            });
            if let Some(key) = params.key("parallel") {
                arguments["api_key"] = serde_json::Value::String(key.to_string());
            }
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "web_search", "arguments": arguments },
            });

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| SearchProviderError::new("parallel", error.to_string()))?;
            let response = client
                .post(MCP_URL)
                .header("Accept", "application/json, text/event-stream")
                .json(&body)
                .send()
                .await
                .map_err(|error| SearchProviderError::new("parallel", error.to_string()))?;
            let status = response.status().as_u16();
            let raw = response
                .text()
                .await
                .map_err(|error| SearchProviderError::new("parallel", error.to_string()))?;
            if status >= 400 {
                return Err(SearchProviderError::with_status(
                    "parallel",
                    super::classify_http_error("parallel", status, &raw)
                        .map(|error| error.message)
                        .unwrap_or_else(|| format!("HTTP {status}")),
                    status,
                ));
            }
            // The MCP server may answer as a data: line of an SSE stream.
            let json = raw
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap_or(&raw);
            let payload: serde_json::Value = serde_json::from_str(json).map_err(|error| {
                SearchProviderError::new("parallel", format!("bad MCP answer: {error}"))
            })?;
            if let Some(error) = payload.get("error") {
                return Err(SearchProviderError::new("parallel", error.to_string()));
            }
            let content = payload
                .get("result")
                .and_then(|result| result.get("content"))
                .and_then(|content| content.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.get("text"))
                .and_then(|text| text.as_str())
                .ok_or_else(|| SearchProviderError::new("parallel", "no results in the answer"))?;
            let sources = parse_sources(content, limit);
            if sources.is_empty() {
                return Err(SearchProviderError::new("parallel", "no result"));
            }
            Ok(SearchResponse {
                provider: "parallel".to_string(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_come_from_the_mcp_text() {
        let payload = r#"{"results":[{"url":"https://example.com/a","title":"A page","excerpts":["An excerpt"],"publish_date":"2026-11-01"}]}"#;
        let sources = parse_sources(payload, 10);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "A page");
        assert_eq!(sources[0].snippet.as_deref(), Some("An excerpt"));
        assert!(sources[0].age_seconds.is_some());
    }

    #[test]
    fn long_queries_become_three_short_searches() {
        let params = SearchParams {
            query: "a very long query about several different topics that must be split into keyword searches".into(),
            parsed: crate::websearch::query::parse_search_query("a very long query about several different topics that must be split into keyword searches"),
            limit: None,
            recency: None,
            keys: super::super::Keys::default(),
            key_env: Default::default(),
            searxng_url: None,
        };
        assert_eq!(search_queries(&params).len(), 3);
    }
}

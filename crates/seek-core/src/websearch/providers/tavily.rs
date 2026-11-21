//! Tavily: the search API of Tavily (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/tavily.ts`.
//!
//! The key is in `TAVILY_API_KEY`, or in the auth file under that name.

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results, date_to_age_seconds,
};

/// The address of the search.
const SEARCH_URL: &str = "https://api.tavily.com/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 5;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The syntax that is sent to Tavily: the quoted phrases and the exclusions.
/// The other directives go into the parameters of the API.
const TAVILY_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
    phrases: true,
    negation: true,
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

/// The Tavily search provider.
pub struct Tavily;

impl SearchProvider for Tavily {
    fn id(&self) -> &'static str {
        "tavily"
    }

    fn label(&self) -> &'static str {
        "Tavily"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params.key("tavily").is_some()
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(key) = params.key("tavily") else {
                return Err(SearchProviderError::new(
                    "tavily",
                    "Tavily needs a key: set TAVILY_API_KEY, or put it in state/auth.json",
                ));
            };
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);

            // The query holds the terms and the phrases; the sites and the
            // dates go into the parameters of the API.
            let query = if params.parsed.has_directives {
                format_query(&params.parsed, &TAVILY_QUERY_SYNTAX)
            } else {
                params.query.trim().to_string()
            };
            if query.is_empty() {
                return Err(SearchProviderError::with_status(
                    "tavily",
                    "Tavily needs search terms: a query of dates only is not supported",
                    400,
                ));
            }

            let mut body = serde_json::json!({
                "query": query,
                "search_depth": "basic",
                "max_results": limit,
                "include_answer": "advanced",
                "include_raw_content": false,
            });
            let include = site_hosts(&params.parsed.sites);
            if !include.is_empty() {
                body["include_domains"] = serde_json::json!(include);
            }
            let exclude = site_hosts(&params.parsed.excluded_sites);
            if !exclude.is_empty() {
                body["exclude_domains"] = serde_json::json!(exclude);
            }
            if let Some(after) = &params.parsed.after {
                body["start_date"] = serde_json::json!(after);
            }
            if let Some(before) = &params.parsed.before {
                body["end_date"] = serde_json::json!(before);
            }
            // The bounds of the dates win over the recency of the search.
            if body.get("start_date").is_none()
                && body.get("end_date").is_none()
                && let Some(recency) = params.recency
            {
                body["time_range"] = serde_json::json!(recency.as_str());
            }

            let client = super::client();

            let response = client
                .post(SEARCH_URL)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {key}"))
                .json(&body)
                .send()
                .await
                .map_err(|error| {
                    SearchProviderError::new("tavily", format!("the search failed: {error}"))
                })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new("tavily", format!("the answer could not be read: {error}"))
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("tavily", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "tavily",
                    format!("Tavily answered {status}: {}", text.trim()),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "tavily",
                    format!("Tavily answered with invalid JSON: {error}"),
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
                        .get("published_date")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    sources.push(SearchSource {
                        title: result
                            .get("title")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(url)
                            .to_string(),
                        url: url.to_string(),
                        snippet: result
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                        age_seconds: published_date.as_deref().and_then(date_to_age_seconds),
                        published_date,
                    });
                    if sources.len() >= limit {
                        break;
                    }
                }
            }
            Ok(SearchResponse {
                provider: "tavily".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hosts_of_the_sites_are_taken_out() {
        let sites = vec![
            "github.com/anthropics".to_string(),
            "github.com".to_string(),
            "example.com".to_string(),
        ];
        assert_eq!(site_hosts(&sites), vec!["github.com", "example.com"]);
    }
}

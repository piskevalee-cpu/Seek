//! Kagi: the search API of Kagi (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/kagi.ts` and `web/kagi.ts`.
//!
//! The key is in `KAGI_API_KEY`, or in the auth file under that name.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{GOOGLE_QUERY_SYNTAX, format_query};
use crate::websearch::types::{
    Recency, SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
    date_to_age_seconds,
};

/// The address of the search.
const SEARCH_URL: &str = "https://kagi.com/api/v1/search";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 40;

/// The lower bound of the window of a recency, as a date.
fn recency_date(recency: Recency) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    let days = match recency {
        Recency::Day => 1,
        Recency::Week => 7,
        Recency::Month => 30,
        Recency::Year => 365,
    };
    let day = (now - days * 86_400) / 86_400;
    crate::websearch::types::iso_from_days(day)
}

/// The Kagi search provider.
pub struct Kagi;

impl SearchProvider for Kagi {
    fn id(&self) -> &'static str {
        "kagi"
    }

    fn label(&self) -> &'static str {
        "Kagi"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params.key("kagi").is_some()
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(key) = params.key("kagi") else {
                return Err(SearchProviderError::new(
                    "kagi",
                    "Kagi needs a key: set KAGI_API_KEY, or put it in state/auth.json",
                ));
            };
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let query = if params.parsed.has_directives {
                format_query(&params.parsed, &GOOGLE_QUERY_SYNTAX)
            } else {
                params.query.trim().to_string()
            };

            let mut body = serde_json::json!({
                "query": query,
                "workflow": "search",
                "limit": limit,
            });
            if let Some(recency) = params.recency {
                body["filters"] = serde_json::json!({ "after": recency_date(recency) });
            }

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("kagi", format!("the client failed: {error}"))
                })?;

            let response = client
                .post(SEARCH_URL)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {key}"))
                .json(&body)
                .send()
                .await
                .map_err(|error| {
                    SearchProviderError::new("kagi", format!("the search failed: {error}"))
                })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new("kagi", format!("the answer could not be read: {error}"))
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("kagi", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "kagi",
                    format!("Kagi answered {status}: {}", text.trim()),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "kagi",
                    format!("Kagi answered with invalid JSON: {error}"),
                    502,
                )
            })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            // The buckets of Kagi: the web pages, the videos, the news and the
            // cards of the encyclopedia.
            let buckets: [(&str, &str); 4] = [
                ("search", ""),
                ("video", "[Video] "),
                ("news", "[News] "),
                ("infobox", "[Info] "),
            ];
            if let Some(data) = data.get("data").and_then(serde_json::Value::as_object) {
                for (bucket, tag) in buckets {
                    let Some(items) = data.get(bucket).and_then(serde_json::Value::as_array) else {
                        continue;
                    };
                    for item in items {
                        let url = item
                            .get("url")
                            .or_else(|| item.get("href"))
                            .or_else(|| item.get("link"))
                            .and_then(serde_json::Value::as_str);
                        let Some(url) = url else { continue };
                        let title = item
                            .get("title")
                            .or_else(|| item.get("name"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(url)
                            .to_string();
                        let published_date = item
                            .get("time")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string);
                        sources.push(SearchSource {
                            title: format!("{tag}{title}"),
                            url: url.to_string(),
                            snippet: item
                                .get("snippet")
                                .or_else(|| item.get("description"))
                                .or_else(|| item.get("summary"))
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string),
                            age_seconds: published_date.as_deref().and_then(date_to_age_seconds),
                            published_date,
                        });
                        if sources.len() >= limit {
                            break;
                        }
                    }
                    if sources.len() >= limit {
                        break;
                    }
                }
            }
            Ok(SearchResponse {
                provider: "kagi".into(),
                sources,
            })
        })
    }
}

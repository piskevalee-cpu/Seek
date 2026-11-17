//! SearXNG: the JSON API of an instance the user points at (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/searxng.ts`.
//!
//! SearXNG is a metasearch engine the user hosts or borrows, so the address of
//! the instance is part of the configuration (`web_search.searxng_url`). The
//! instance may ask for a bearer token or for RFC 7617 Basic credentials; both
//! are read from the environment or from `state/auth.json`, never from
//! `config.toml` (SPEC 5).
//!
//! External bangs (`!!g`) are removed before the request: SearXNG answers them
//! with an HTTP redirect even for JSON. Single bangs (`!ddg`, `!images`) are
//! kept: the instance resolves and removes them server-side.

use std::time::Duration;

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{GOOGLE_QUERY_SYNTAX, format_scraper_query};
use crate::websearch::types::{
    Recency, SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
    date_to_age_seconds,
};

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 20;

/// The name of the bearer token, and of the Basic credentials, in the
/// environment or in `state/auth.json`.
const TOKEN_VARIABLE: &str = "SEARXNG_TOKEN";
const BASIC_USERNAME_VARIABLE: &str = "SEARXNG_BASIC_USERNAME";
const BASIC_PASSWORD_VARIABLE: &str = "SEARXNG_BASIC_PASSWORD";

/// The window of a recency in the vocabulary of SearXNG.
///
/// SearXNG knows a day, a month and a year; a week becomes a month.
fn recency_to_time_range(recency: Recency) -> &'static str {
    match recency {
        Recency::Day => "day",
        Recency::Week => "month",
        Recency::Month => "month",
        Recency::Year => "year",
    }
}

/// The credential of a search, when the instance asks for one.
enum Auth {
    Basic(String),
    Bearer(String),
}

/// The credential the user configured, from the environment or the auth file.
fn find_auth(params: &SearchParams) -> Result<Option<Auth>, SearchProviderError> {
    let username = params.keys.get(BASIC_USERNAME_VARIABLE);
    let password = params.keys.get(BASIC_PASSWORD_VARIABLE);
    if username.is_some() || password.is_some() {
        let (Some(username), Some(password)) = (username, password) else {
            return Err(SearchProviderError::new(
                "searxng",
                "SearXNG Basic auth needs both SEARXNG_BASIC_USERNAME and \
                 SEARXNG_BASIC_PASSWORD",
            ));
        };
        if username.contains(':') {
            return Err(SearchProviderError::new(
                "searxng",
                "the username of SearXNG Basic auth cannot contain ':'",
            ));
        }
        if username.chars().any(char::is_control) || password.chars().any(char::is_control) {
            return Err(SearchProviderError::new(
                "searxng",
                "the credentials of SearXNG Basic auth must not contain control characters",
            ));
        }
        return Ok(Some(Auth::Basic(base64(&format!("{username}:{password}")))));
    }
    Ok(params
        .keys
        .get(TOKEN_VARIABLE)
        .map(|token| Auth::Bearer(token.to_string())))
}

/// The `Authorization` header of the credential.
fn authorization(auth: &Option<Auth>) -> Option<String> {
    match auth {
        Some(Auth::Basic(value)) => Some(format!("Basic {value}")),
        Some(Auth::Bearer(value)) => Some(format!("Bearer {value}")),
        None => None,
    }
}

/// Standard base64 with padding, which is what RFC 7617 asks for.
fn base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied();
        let third = chunk.get(2).copied();
        out.push(ALPHABET[(first >> 2) as usize] as char);
        out.push(ALPHABET[(((first & 0b11) << 4) | (second.unwrap_or(0) >> 4)) as usize] as char);
        out.push(match second {
            Some(second) => {
                ALPHABET[(((second & 0b1111) << 2) | (third.unwrap_or(0) >> 6)) as usize] as char
            }
            None => '=',
        });
        out.push(match third {
            Some(third) => ALPHABET[(third & 0b11_1111) as usize] as char,
            None => '=',
        });
    }
    out
}

/// Remove the external bangs, which the instance answers with a redirect.
fn strip_external_bangs(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|part| !part.starts_with("!!"))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// The text of a field of a result.
fn text_of(value: Option<&serde_json::Value>) -> Option<String> {
    let text = value.and_then(serde_json::Value::as_str)?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.to_string())
}

/// The SearXNG search provider.
pub struct SearXng;

impl SearchProvider for SearXng {
    fn id(&self) -> &'static str {
        "searxng"
    }

    fn label(&self) -> &'static str {
        "SearXNG"
    }

    fn is_available(&self, params: &SearchParams) -> bool {
        params
            .searxng_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let Some(endpoint) = params
                .searxng_url
                .as_deref()
                .map(str::trim)
                .filter(|url| !url.is_empty())
            else {
                return Err(SearchProviderError::new(
                    "searxng",
                    "SearXNG needs the address of an instance: set web_search.searxng_url in \
                     config.toml",
                ));
            };
            let endpoint = endpoint.trim_end_matches('/');
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let query =
                format_scraper_query(&params.query, Some(&params.parsed), &GOOGLE_QUERY_SYNTAX);
            let query = strip_external_bangs(&query);
            let auth = find_auth(params)?;

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| {
                    SearchProviderError::new("searxng", format!("the client failed: {error}"))
                })?;

            let url = format!("{endpoint}/search");
            let mut request = client.get(&url).query(&[
                ("q", query.as_str()),
                ("format", "json"),
                ("pageno", "1"),
            ]);
            if let Some(recency) = params.recency {
                request = request.query(&[("time_range", recency_to_time_range(recency))]);
            }
            if let Some(language) = params.parsed.lang.as_deref() {
                request = request.query(&[("language", language)]);
            }
            request = request.header("Accept", "application/json");
            if let Some(authorization) = authorization(&auth) {
                request = request.header("Authorization", authorization);
            }

            let response = request.send().await.map_err(|error| {
                SearchProviderError::new("searxng", format!("the search failed: {error}"))
            })?;
            let status = response.status();
            let text = response.text().await.map_err(|error| {
                SearchProviderError::new(
                    "searxng",
                    format!("the answer could not be read: {error}"),
                )
            })?;
            if !status.is_success() {
                if let Some(error) = classify_http_error("searxng", status.as_u16(), &text) {
                    return Err(error);
                }
                return Err(SearchProviderError::with_status(
                    "searxng",
                    format!("SearXNG answered {status}: {text}"),
                    status.as_u16(),
                ));
            }
            let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                SearchProviderError::with_status(
                    "searxng",
                    format!(
                        "SearXNG answered with invalid JSON: {error}. The instance has to allow \
                         the json format of its search API"
                    ),
                    500,
                )
            })?;

            let mut sources: Vec<SearchSource> = Vec::new();
            if let Some(results) = data.get("results").and_then(serde_json::Value::as_array) {
                for result in results {
                    let Some(url) = text_of(result.get("url")) else {
                        continue;
                    };
                    let published_date = text_of(result.get("publishedDate"))
                        .or_else(|| text_of(result.get("published_date")));
                    sources.push(SearchSource {
                        title: text_of(result.get("title")).unwrap_or_else(|| url.clone()),
                        url,
                        snippet: text_of(result.get("content"))
                            .or_else(|| text_of(result.get("snippet"))),
                        age_seconds: published_date.as_deref().and_then(date_to_age_seconds),
                        published_date,
                    });
                    if sources.len() >= limit {
                        break;
                    }
                }
            }

            if sources.is_empty()
                && let Some(engines) = data
                    .get("unresponsive_engines")
                    .and_then(serde_json::Value::as_array)
                    .filter(|engines| !engines.is_empty())
            {
                let failures = engines
                    .iter()
                    .map(|engine| {
                        engine
                            .as_array()
                            .map(|pair| {
                                pair.iter()
                                    .filter_map(serde_json::Value::as_str)
                                    .collect::<Vec<&str>>()
                                    .join(": ")
                            })
                            .unwrap_or_else(|| engine.to_string())
                    })
                    .collect::<Vec<String>>()
                    .join("; ");
                return Err(SearchProviderError::with_status(
                    "searxng",
                    format!("SearXNG returned no result; its engines failed: {failures}"),
                    503,
                ));
            }

            Ok(SearchResponse {
                provider: "searxng".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basic_credential_is_base64() {
        assert_eq!(base64("user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64("a"), "YQ==");
        assert_eq!(base64("ab"), "YWI=");
    }

    #[test]
    fn the_external_bangs_are_removed() {
        assert_eq!(strip_external_bangs("rust !!g book"), "rust book");
        assert_eq!(strip_external_bangs("rust !ddg book"), "rust !ddg book");
    }

    #[test]
    fn a_week_becomes_a_month() {
        assert_eq!(recency_to_time_range(Recency::Week), "month");
        assert_eq!(recency_to_time_range(Recency::Day), "day");
    }
}

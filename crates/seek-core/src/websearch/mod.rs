//! The web search (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner): `web/search/`.
//!
//! One tool, many engines. The chain of the providers is walked in order: the
//! first one that answers wins, and the ones that failed are named in a note
//! so the model knows whose results it is reading.
//!
//! The query of the model is split into its terms and its constraints before
//! it goes anywhere ([`query`]): `site:`, `before:`/`after:`, `inurl:`,
//! `filetype:`, the quoted phrases, the `OR` groups and the `-exclusions` are
//! taken out, given to the engines that understand them as parameters, and
//! enforced on the results that come back.
//!
//! The keys of the providers never go into `config.toml`: the configuration
//! names the environment variable the key is in, and the key itself is read
//! from the environment or from `state/auth.json` (SPEC 5).

pub mod html;
pub mod providers;
pub mod query;
pub mod types;

use std::collections::BTreeMap;

use crate::tool::{Risk, ToolError, ToolSchema};
use crate::truncate::cap_output;

pub use providers::{Keys, SearchParams};
pub use query::{GOOGLE_QUERY_SYNTAX, StructuredQuery};
pub use types::{Recency, SearchProviderError, SearchResponse, SearchSource};

/// How long a search may take.
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The chain of the providers when the configuration names none.
///
/// The engines that need no key come first: DuckDuckGo, then Mojeek, then
/// Startpage, then Ecosia.
pub const DEFAULT_CHAIN: [&str; 4] = ["duckduckgo", "mojeek", "startpage", "ecosia"];

/// The settings of the web search: the chain of the providers, the address of a
/// SearXNG instance, and the environment variables the keys are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The providers of the chain, in order.
    pub providers: Vec<String>,
    /// The address of a SearXNG instance.
    pub searxng_url: Option<String>,
    /// The name of the environment variable the key of a provider is in, by the
    /// id of the provider.
    pub key_env: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            providers: DEFAULT_CHAIN.iter().map(|id| id.to_string()).collect(),
            searxng_url: None,
            key_env: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// The settings out of the configuration.
    pub fn from_config(config: &crate::config::WebSearchConfig) -> Self {
        let providers = match &config.providers {
            Some(providers) if !providers.is_empty() => providers.clone(),
            _ => DEFAULT_CHAIN.iter().map(|id| id.to_string()).collect(),
        };
        Settings {
            providers,
            searxng_url: config.searxng_url.clone(),
            key_env: config.key_env.clone(),
        }
    }
}

/// Searches the web.
#[derive(Debug, Clone)]
pub struct WebSearchTool {
    settings: Settings,
    keys: Keys,
}

impl WebSearchTool {
    /// The tool with the default chain.
    pub fn new(settings: Settings, auth: &crate::config::Auth) -> Self {
        WebSearchTool {
            settings,
            keys: Keys::new(auth),
        }
    }

    /// The tool with a given set of keys.
    pub fn with_keys(settings: Settings, keys: Keys) -> Self {
        WebSearchTool { settings, keys }
    }

    /// Search the web and return the results, as the model reads them.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<usize>,
        recency: Option<Recency>,
    ) -> Result<String, ToolError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput("the query is empty".into()));
        }
        let parsed = query::parse_search_query(query);
        let params = SearchParams {
            query: query.to_string(),
            parsed,
            limit,
            recency,
            keys: self.keys.clone(),
            key_env: self.settings.key_env.clone(),
            searxng_url: self.settings.searxng_url.clone(),
        };

        let mut failures: Vec<(String, String)> = Vec::new();
        for id in &self.settings.providers {
            let Some(provider) = providers::provider(id) else {
                continue;
            };
            if !provider.is_available(&params) {
                continue;
            }
            let label = provider.label();
            match provider.search(&params).await {
                Ok(response) if response.sources.is_empty() => {
                    failures.push((label.to_string(), "no result".to_string()));
                }
                Ok(response) => {
                    // The constraints the engine could not honour are enforced
                    // on the sources that came back. A constraint that would
                    // leave nothing is relaxed, and the model is told so.
                    let mut notes: Vec<String> = Vec::new();
                    if !failures.is_empty() {
                        notes.push(format!(
                            "answered by {label} after {}",
                            failures
                                .iter()
                                .map(|(name, reason)| format!("{name} failed ({reason})"))
                                .collect::<Vec<String>>()
                                .join(" and ")
                        ));
                    }
                    let mut sources = response.sources;
                    if params.parsed.has_constraints && !sources.is_empty() {
                        let filtered = query::apply_query_constraints(&sources, &params.parsed);
                        if filtered.sources.len() != sources.len() {
                            sources = filtered.sources;
                        }
                        for label in filtered.dropped {
                            notes.push(format!(
                                "no result matched `{label}`; the constraint was relaxed"
                            ));
                        }
                    }
                    return Ok(cap_output(&format_results(&sources, &notes)));
                }
                Err(error) => {
                    failures.push((label.to_string(), error.message));
                }
            }
        }

        if failures.is_empty() {
            return Err(ToolError::Failed(
                "no web search provider is configured".into(),
            ));
        }
        let message = failures
            .iter()
            .map(|(name, reason)| format!("{name}: {reason}"))
            .collect::<Vec<String>>()
            .join("; ");
        Err(ToolError::Failed(format!("the search failed — {message}")))
    }
}

/// The results, in the shape the model reads them.
fn format_results(sources: &[SearchSource], notes: &[String]) -> String {
    let mut out = String::new();
    for note in notes {
        out.push_str(&format!("Note: {note}\n"));
    }
    if !notes.is_empty() {
        out.push('\n');
    }
    for (index, source) in sources.iter().enumerate() {
        let age = source
            .age_seconds
            .map(format_age)
            .or_else(|| source.published_date.clone());
        let age = age.map(|age| format!(" ({age})")).unwrap_or_default();
        out.push_str(&format!("[{}] {}{}\n", index + 1, source.title, age));
        out.push_str(&format!("    {}\n", source.url));
        if let Some(snippet) = &source.snippet
            && !snippet.is_empty()
        {
            out.push_str(&format!("    {}\n", snippet));
        }
    }
    out
}

/// An age in words, as the model reads it.
fn format_age(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3_599 => {
            let minutes = seconds / 60;
            format!(
                "{minutes} minute{} ago",
                if minutes == 1 { "" } else { "s" }
            )
        }
        3_600..=86_399 => {
            let hours = seconds / 3_600;
            format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" })
        }
        86_400..=2_591_999 => {
            let days = seconds / 86_400;
            format!("{days} day{} ago", if days == 1 { "" } else { "s" })
        }
        _ => {
            let months = seconds / 2_592_000;
            format!("{months} month{} ago", if months == 1 { "" } else { "s" })
        }
    }
}

/// The tool the model calls.
impl crate::tool::Tool for WebSearchTool {
    fn schema(&self) -> ToolSchema {
        web_search_schema()
    }

    fn call<'a>(
        &'a self,
        input: serde_json::Value,
    ) -> crate::tool::BoxFuture<'a, Result<crate::event::ToolOutput, ToolError>> {
        Box::pin(async move {
            let query = input
                .get("query")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| ToolError::InvalidInput("the query is missing".into()))?
                .to_string();
            let limit = input
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .map(|number| number as usize);
            let recency = input
                .get("recency")
                .and_then(serde_json::Value::as_str)
                .and_then(Recency::parse);
            let content = self.search(&query, limit, recency).await?;
            Ok(crate::event::ToolOutput {
                id: String::new(),
                name: "web_search".into(),
                content,
                is_error: false,
                duration: std::time::Duration::ZERO,
            })
        })
    }
}

/// The schema of the `web_search` tool.
pub fn web_search_schema() -> ToolSchema {
    ToolSchema::new(
        "web_search",
        "Search the web. The query may hold the operators of the search engines: site:, \
         intitle:, inurl:, filetype:, before:, after:, a \"quoted phrase\", a OR b, and -word to \
         leave something out. The recency keeps the results fresh.",
        r#"{"type":"object","properties":{"query":{"type":"string"},"recency":{"type":"string","enum":["day","week","month","year"]},"limit":{"type":"integer"}},"required":["query"]}"#,
        Risk::Network,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_results_are_shaped_for_the_model() {
        let sources = vec![SearchSource {
            title: "Rust".into(),
            url: "https://rust-lang.org/".into(),
            snippet: Some("A fast language.".into()),
            age_seconds: Some(7_200),
            ..Default::default()
        }];
        let text = format_results(&sources, &[]);
        assert!(text.contains("[1] Rust (2 hours ago)"), "{text}");
        assert!(text.contains("    https://rust-lang.org/"), "{text}");
        assert!(text.contains("    A fast language."), "{text}");
    }

    #[test]
    fn a_note_comes_first() {
        let text = format_results(&[], &["answered by DuckDuckGo after Mojeek failed".into()]);
        assert!(text.starts_with("Note: answered by DuckDuckGo"), "{text}");
    }

    #[test]
    fn the_schema_carries_the_operators() {
        let schema = web_search_schema();
        assert!(schema.parameters.contains("recency"));
        assert!(schema.parameters.contains("query"));
    }

    #[test]
    fn the_settings_come_from_the_configuration() {
        let config = crate::config::WebSearchConfig {
            providers: Some(vec!["brave".into(), "public".into()]),
            searxng_url: Some("https://searx.example.org".into()),
            key_env: std::collections::BTreeMap::from([("brave".into(), "MY_BRAVE".into())]),
        };
        let settings = Settings::from_config(&config);
        assert_eq!(settings.providers, ["brave", "public"]);
        assert_eq!(
            settings.searxng_url.as_deref(),
            Some("https://searx.example.org")
        );
        assert_eq!(
            settings.key_env.get("brave").map(String::as_str),
            Some("MY_BRAVE")
        );

        // An empty section leaves the default chain in place.
        let settings = Settings::from_config(&crate::config::WebSearchConfig::default());
        assert_eq!(settings.providers, DEFAULT_CHAIN);
    }
}

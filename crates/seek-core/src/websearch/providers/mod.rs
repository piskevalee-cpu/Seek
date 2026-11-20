//! The providers of a web search (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner): `web/search/providers/`.
//!
//! Every provider answers a [`SearchParams`] with the sources it found. The
//! ones that need a key read it from the environment, or from `state/auth.json`
//! under the name of the environment variable (SPEC 5: a key never goes into
//! `config.toml`).

pub mod brave;
pub mod duckduckgo;
pub mod ecosia;
pub mod exa;
pub mod firecrawl;
pub mod hackernews;
pub mod jina;
pub mod kagi;
pub mod mojeek;
pub mod parallel;
pub mod public;
pub mod searxng;
pub mod startpage;
pub mod tavily;

use std::collections::BTreeMap;

use crate::tool::BoxFuture;

use super::query::StructuredQuery;
use super::types::{Recency, SearchProviderError, SearchResponse};

/// The keys of the user: the environment first, then `state/auth.json`.
///
/// The name of the environment variable is the name of the entry in the auth
/// file, so a key can be given either way.
#[derive(Debug, Clone, Default)]
pub struct Keys {
    entries: BTreeMap<String, String>,
}

impl Keys {
    /// The keys of the environment and of the auth file.
    ///
    /// An environment variable wins over the auth file, so a key can be
    /// overridden for one run without editing the file.
    pub fn new(auth: &crate::config::Auth) -> Self {
        let mut entries = BTreeMap::new();
        for (name, value) in std::env::vars() {
            if !value.is_empty() {
                entries.insert(name, value);
            }
        }
        for (name, value) in &auth.keys {
            if !value.is_empty() {
                entries.entry(name.clone()).or_insert_with(|| value.clone());
            }
        }
        Keys { entries }
    }

    /// The keys of the environment alone.
    pub fn from_env() -> Self {
        let mut entries = BTreeMap::new();
        for (name, value) in std::env::vars() {
            if !value.is_empty() {
                entries.entry(name).or_insert(value);
            }
        }
        Keys { entries }
    }

    /// The key in the environment variable of that name, or in the auth file.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }
}

/// What a provider needs to answer a search.
#[derive(Debug, Clone)]
pub struct SearchParams {
    /// The query as the model wrote it.
    pub query: String,
    /// The query, split into its terms and its constraints.
    pub parsed: StructuredQuery,
    /// How many results the model asked for.
    pub limit: Option<usize>,
    /// How fresh the results have to be.
    pub recency: Option<Recency>,
    /// The keys of the user.
    pub keys: Keys,
    /// The name of the environment variable the key of a provider is in, by the
    /// id of the provider. The default of [`key_env_of`] applies without an
    /// entry.
    pub key_env: BTreeMap<String, String>,
    /// The address of a SearXNG instance, when there is one.
    pub searxng_url: Option<String>,
}

impl SearchParams {
    /// The key of a provider, from the environment or the auth file.
    ///
    /// The configuration may point at another environment variable than the
    /// default one, so the mapping of [`key_env_of`] is only the fallback.
    pub fn key(&self, id: &str) -> Option<&str> {
        let name = self
            .key_env
            .get(id)
            .map(String::as_str)
            .or_else(|| key_env_of(id))?;
        self.keys.get(name)
    }
}

/// A provider of search results.
pub trait SearchProvider: Send + Sync {
    /// The id of the provider: the name in the configuration.
    fn id(&self) -> &'static str;

    /// The name of the provider, for the user.
    fn label(&self) -> &'static str;

    /// Whether the provider can answer right now: a key-free one always can,
    /// a keyed one only when its key is there.
    fn is_available(&self, params: &SearchParams) -> bool;

    /// Answer a search.
    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>>;
}

/// The provider of that id.
///
/// The registry is the same as the one of the reference harness: the id is the
/// name the user writes in the configuration.
pub fn provider(id: &str) -> Option<Box<dyn SearchProvider>> {
    Some(match id {
        "brave" => Box::new(brave::Brave),
        "duckduckgo" => Box::new(duckduckgo::DuckDuckGo),
        "ecosia" => Box::new(ecosia::Ecosia),
        "exa" => Box::new(exa::Exa),
        "firecrawl" => Box::new(firecrawl::Firecrawl),
        "hackernews" => Box::new(hackernews::HackerNews),
        "jina" => Box::new(jina::Jina),
        "kagi" => Box::new(kagi::Kagi),
        "mojeek" => Box::new(mojeek::Mojeek),
        "parallel" => Box::new(parallel::Parallel),
        "public" => Box::new(public::PublicWeb),
        "searxng" => Box::new(searxng::SearXng),
        "startpage" => Box::new(startpage::Startpage),
        "tavily" => Box::new(tavily::Tavily),
        _ => return None,
    })
}

/// The ids of the providers, in the order they are offered.
pub const ENGINES: [&str; 14] = [
    "parallel",
    "duckduckgo",
    "startpage",
    "mojeek",
    "ecosia",
    "hackernews",
    "searxng",
    "public",
    "brave",
    "tavily",
    "exa",
    "jina",
    "kagi",
    "firecrawl",
];

/// The name of a provider, for the user.
pub fn label_of(id: &str) -> &'static str {
    match id {
        "brave" => "Brave",
        "duckduckgo" => "DuckDuckGo",
        "ecosia" => "Ecosia",
        "exa" => "Exa",
        "firecrawl" => "Firecrawl",
        "hackernews" => "Hacker News",
        "jina" => "Jina",
        "kagi" => "Kagi",
        "mojeek" => "Mojeek",
        "parallel" => "Parallel",
        "public" => "Public Web",
        "searxng" => "SearXNG",
        "startpage" => "Startpage",
        "tavily" => "Tavily",
        _ => "Web search",
    }
}

/// The environment variable the key of a provider is in.
pub fn key_env_of(id: &str) -> Option<&'static str> {
    Some(match id {
        "brave" => "BRAVE_API_KEY",
        "exa" => "EXA_API_KEY",
        "firecrawl" => "FIRECRAWL_API_KEY",
        "jina" => "JINA_API_KEY",
        "kagi" => "KAGI_API_KEY",
        "tavily" => "TAVILY_API_KEY",
        _ => return None,
    })
}

/// Whether the provider needs a key to answer.
pub fn needs_key(id: &str) -> bool {
    key_env_of(id).is_some()
}

/// The body of a form, url-encoded by hand.
///
/// The crate has no form feature, and the encoding of a form is short: the
/// space becomes a `+`, and everything else is escaped.
pub fn encode_form(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<String>>()
        .join("&")
}

/// Escape a text for a form.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A quota or an authentication failure, as the providers report it.
///
/// The messages are short and name the provider, so the chain can move on and
/// the note that is left for the user stays readable.
pub fn classify_http_error(id: &str, status: u16, body: &str) -> Option<SearchProviderError> {
    let label = label_of(id);
    let lower = body.to_lowercase();
    if lower.contains("credits exhausted")
        || lower.contains("credit exhausted")
        || lower.contains("credits exceeded")
        || lower.contains("quota")
        || lower.contains("insufficient")
    {
        return Some(SearchProviderError::with_status(
            id,
            format!("{label}: the credits are exhausted"),
            status,
        ));
    }
    match status {
        402 => Some(SearchProviderError::with_status(
            id,
            format!("{label}: the credits are exhausted (402)"),
            status,
        )),
        401 => Some(SearchProviderError::with_status(
            id,
            format!("{label}: the key was refused (401)"),
            status,
        )),
        403 => Some(SearchProviderError::with_status(
            id,
            format!("{label}: the key was refused (403)"),
            status,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_has_a_provider() {
        for id in ENGINES {
            assert!(provider(id).is_some(), "{id} has no provider");
        }
    }

    #[test]
    fn an_unknown_engine_has_no_provider() {
        assert!(provider("nope").is_none());
    }

    #[test]
    fn the_key_free_engines_need_no_key() {
        for id in [
            "duckduckgo",
            "startpage",
            "mojeek",
            "ecosia",
            "hackernews",
            "public",
        ] {
            assert!(!needs_key(id), "{id} should not need a key");
        }
        assert!(needs_key("brave"));
    }

    #[test]
    fn a_quota_failure_is_named() {
        let error =
            classify_http_error("brave", 429, "Your monthly quota is exceeded").expect("named");
        assert!(error.message.contains("credits"), "{}", error.message);
    }

    #[test]
    fn an_unauthorized_failure_is_named() {
        let error = classify_http_error("brave", 401, "invalid key").expect("named");
        assert!(error.message.contains("401"), "{}", error.message);
    }
}

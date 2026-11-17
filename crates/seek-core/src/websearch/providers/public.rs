//! Public Web: one search over the engines that need no key (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/public.ts`.
//!
//! The aggregate fans out to every credential-free engine at once and merges
//! what comes back: the same page found by two engines ranks above a page found
//! by one, and the best rank of an engine breaks the ties. Engine order breaks
//! the rest, so the order of the results does not depend on which request
//! finished first.
//!
//! A slow engine never holds the answer: past the soft deadline the aggregate
//! returns what it has, and past the hard deadline it returns even if that is
//! nothing. The provider is explicit-only: a chain that names it chose the
//! fan-out, rather than falling back to it.

use std::collections::HashMap;
use std::time::Duration;

use tokio::time::Instant;

use super::{SearchParams, SearchProvider, label_of, provider};
use crate::tool::BoxFuture;
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results,
};

/// The credential-free engines, in the order that breaks the ties of the merge.
///
/// The engines backed by a Google index come first; Mojeek's index of its own
/// settles the remaining ties.
const PUBLIC_ENGINES: [&str; 4] = ["startpage", "duckduckgo", "ecosia", "mojeek"];

/// An aggregate needs breadth, so it takes more results than one engine.
const DEFAULT_RESULTS: usize = 15;
/// The most results a search gives back.
const MAX_RESULTS: usize = 30;

/// Past this point the aggregate returns as soon as one engine has answered.
const SOFT_DEADLINE: Duration = Duration::from_secs(5);
/// Past this point it returns whatever it has, even nothing.
const HARD_DEADLINE: Duration = Duration::from_secs(30);

/// One address, and the engines that returned it.
struct MergedSource {
    source: SearchSource,
    /// How many engines returned the address: the first ranking signal.
    engines: usize,
    /// The best (lowest) rank any engine gave it.
    best_rank: usize,
    /// The place of the first sighting, which keeps the merge deterministic.
    order: usize,
}

/// The accumulator of the merge.
#[derive(Default)]
struct Merged {
    entries: Vec<MergedSource>,
    index: HashMap<String, usize>,
}

impl Merged {
    /// Merge one engine's ranked results.
    fn add(&mut self, sources: &[SearchSource]) {
        for (rank, source) in sources.iter().enumerate() {
            let key = dedup_key(&source.url);
            let Some(&at) = self.index.get(&key) else {
                let order = self.entries.len();
                self.index.insert(key, order);
                self.entries.push(MergedSource {
                    source: source.clone(),
                    engines: 1,
                    best_rank: rank,
                    order,
                });
                continue;
            };
            let entry = &mut self.entries[at];
            entry.engines += 1;
            if rank < entry.best_rank {
                entry.best_rank = rank;
                entry.source.title = source.title.clone();
                entry.source.url = source.url.clone();
            }
            // The most informative snippet wins, whichever engine ranked the
            // page best.
            if source.snippet.as_ref().map(String::len).unwrap_or(0)
                > entry.source.snippet.as_ref().map(String::len).unwrap_or(0)
            {
                entry.source.snippet = source.snippet.clone();
            }
            if entry.source.published_date.is_none() {
                entry.source.published_date = source.published_date.clone();
            }
            if entry.source.age_seconds.is_none() {
                entry.source.age_seconds = source.age_seconds;
            }
        }
    }

    /// The merged results, ranked and cut to the number of results.
    fn into_sources(mut self, limit: usize) -> Vec<SearchSource> {
        self.entries.sort_by(|a, b| {
            b.engines
                .cmp(&a.engines)
                .then(a.best_rank.cmp(&b.best_rank))
                .then(a.order.cmp(&b.order))
        });
        self.entries
            .into_iter()
            .take(limit)
            .map(|entry| entry.source)
            .collect()
    }
}

/// The key that makes two engines agree about the same page.
///
/// The engines disagree exactly on the variations of an address: the case of
/// the host, a leading `www.`, the trailing slash of a path and the fragment.
fn dedup_key(raw_url: &str) -> String {
    let Ok(url) = reqwest::Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let host = url.host_str().unwrap_or_default().to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let mut path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    match url.query() {
        Some(query) => format!("{host}{path}?{query}"),
        None => format!("{host}{path}"),
    }
}

/// The Public Web aggregate.
pub struct PublicWeb;

impl SearchProvider for PublicWeb {
    fn id(&self) -> &'static str {
        "public"
    }

    fn label(&self) -> &'static str {
        "Public Web"
    }

    /// Available when the chain names it.
    ///
    /// The aggregate is never part of the default chain: a chain that names it
    /// chose the fan-out over one engine.
    fn is_available(&self, _params: &SearchParams) -> bool {
        true
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let limit = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let soft = Instant::now() + SOFT_DEADLINE;
            let hard = Instant::now() + HARD_DEADLINE;

            // Each engine runs in a task of its own; the channel collects them
            // as they settle, and the deadlines decide when to stop waiting.
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
            for (index, id) in PUBLIC_ENGINES.iter().enumerate() {
                let sender = sender.clone();
                let params = params.clone();
                handles.push(tokio::spawn(async move {
                    let result = match provider(id) {
                        Some(provider) if provider.is_available(&params) => {
                            provider.search(&params).await
                        }
                        _ => Err(SearchProviderError::new(
                            id,
                            format!("{id} is not available"),
                        )),
                    };
                    let _ = sender.send((index, id, result));
                }));
            }
            drop(sender);

            let mut responses: Vec<Option<SearchResponse>> =
                (0..PUBLIC_ENGINES.len()).map(|_| None).collect();
            let mut failures: Vec<(&'static str, String)> = Vec::new();
            let mut remaining = PUBLIC_ENGINES.len();
            let mut answered = false;
            let mut deadline = soft;

            while remaining > 0 {
                tokio::select! {
                    message = receiver.recv() => {
                        let Some((index, id, result)) = message else { break };
                        remaining -= 1;
                        match result {
                            Ok(response) => {
                                answered = true;
                                responses[index] = Some(response);
                            }
                            Err(error) => {
                                failures.push((label_of(id), error.message));
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        if answered || deadline >= hard {
                            break;
                        }
                        // Nothing has answered yet: keep waiting for the first
                        // success, but never past the hard deadline.
                        deadline = hard;
                    }
                }
            }
            for handle in handles {
                handle.abort();
            }

            let mut merged = Merged::default();
            for response in responses.into_iter().flatten() {
                merged.add(&response.sources);
            }
            let sources = merged.into_sources(limit);

            if sources.is_empty() && !answered {
                let message = if failures.is_empty() {
                    "no public engine answered".to_string()
                } else {
                    failures
                        .iter()
                        .map(|(name, reason)| format!("{name}: {reason}"))
                        .collect::<Vec<String>>()
                        .join("; ")
                };
                return Err(SearchProviderError::with_status(
                    "public",
                    format!("every public engine failed — {message}"),
                    503,
                ));
            }

            Ok(SearchResponse {
                provider: "public".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(url: &str) -> SearchSource {
        SearchSource {
            title: url.to_string(),
            url: url.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn the_variations_of_an_address_are_one_key() {
        assert_eq!(
            dedup_key("https://WWW.Rust-Lang.org/book/#intro"),
            dedup_key("https://rust-lang.org/book/")
        );
        assert_eq!(dedup_key("https://rust-lang.org/"), "rust-lang.org/");
    }

    #[test]
    fn the_consensus_outranks_a_better_rank() {
        let mut merged = Merged::default();
        merged.add(&[source("https://a/"), source("https://b/")]);
        merged.add(&[source("https://b/"), source("https://c/")]);
        let sources = merged.into_sources(10);
        assert_eq!(sources[0].url, "https://b/");
        assert_eq!(sources[1].url, "https://a/");
        assert_eq!(sources[2].url, "https://c/");
    }

    #[test]
    fn the_longest_snippet_wins() {
        let mut merged = Merged::default();
        let mut first = source("https://a/");
        first.snippet = Some("short".into());
        let mut second = source("https://a/");
        second.snippet = Some("a much longer preview".into());
        merged.add(&[first]);
        merged.add(&[second]);
        let sources = merged.into_sources(10);
        assert_eq!(sources[0].snippet.as_deref(), Some("a much longer preview"));
    }
}

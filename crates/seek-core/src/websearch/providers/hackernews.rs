//! Hacker News: the public search API of Algolia (SPEC 9.3).
//!
//! Ported from the oh-my-pi harness (MIT, (c) Mario Zechner):
//! `web/search/providers/hackernews.ts`.
//!
//! The search API of Hacker News indexes every story and every comment; the
//! general engines index the threads of Hacker News late and thinly.

use super::{SearchParams, SearchProvider, classify_http_error};
use crate::tool::BoxFuture;
use crate::websearch::query::{QuerySyntax, format_query};
use crate::websearch::types::{
    SearchProviderError, SearchResponse, SearchSource, clamp_num_results, date_to_age_seconds,
};

/// The address of the search API.
const SEARCH_URL: &str = "https://hn.algolia.com/api/v1/search";
/// The address of a discussion.
const ITEM_URL: &str = "https://news.ycombinator.com/item?id=";

/// How many results a search gives back when the model does not say.
const DEFAULT_RESULTS: usize = 10;
/// The most results a search gives back.
const MAX_RESULTS: usize = 30;

/// The syntax of the query of Algolia.
///
/// Algolia reads the quoted phrases and the `-` exclusions. It has no `OR` and
/// it wants every word, so the alternatives go into `optionalWords`.
const HN_QUERY_SYNTAX: QuerySyntax = QuerySyntax {
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

/// The keywords of a search: the terms, and the values of the `intitle:`,
/// `intext:` and `inurl:` directives, which Algolia matches in the title, in
/// the text and in the link at once.
fn keyword_query(params: &SearchParams) -> String {
    let mut parsed = params.parsed.clone();
    let scoped: Vec<crate::websearch::query::QueryTerm> = parsed
        .in_title
        .iter()
        .chain(parsed.in_text.iter())
        .chain(parsed.in_url.iter())
        .map(|text| crate::websearch::query::QueryTerm {
            text: text.clone(),
            ..Default::default()
        })
        .collect();
    parsed.terms.extend(scoped);
    if parsed.terms.is_empty() {
        return String::new();
    }
    format_query(&parsed, &HN_QUERY_SYNTAX)
}

/// The bounds of the date, in the seconds of the epoch.
fn date_filters(params: &SearchParams) -> Vec<String> {
    let mut filters: Vec<String> = Vec::new();
    let start = params.parsed.after.clone().or_else(|| {
        // Without a `before:`, the recency of the search sets the lower bound.
        params
            .recency
            .filter(|_| params.parsed.before.is_none())
            .map(|recency| recency.after_date())
    });
    if let Some(start) = start
        && let Some(seconds) = crate::websearch::query::parse_iso_seconds(&start)
    {
        filters.push(format!("created_at_i>={seconds}"));
    }
    if let Some(before) = &params.parsed.before
        && let Some(seconds) = crate::websearch::query::parse_iso_seconds(before)
    {
        filters.push(format!("created_at_i<{seconds}"));
    }
    filters
}

/// One hit of the search API.
#[derive(Debug, Default)]
struct Hit {
    object_id: Option<String>,
    author: Option<String>,
    created_at: Option<String>,
    title: Option<String>,
    url: Option<String>,
    story_text: Option<String>,
    points: Option<i64>,
    num_comments: Option<i64>,
    comment_text: Option<String>,
    story_title: Option<String>,
}

impl Hit {
    /// Read a hit out of the JSON of the API.
    fn read(value: &serde_json::Value) -> Option<Hit> {
        let object = value.as_object()?;
        let text = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let number = |key: &str| object.get(key).and_then(serde_json::Value::as_i64);
        Some(Hit {
            object_id: text("objectID"),
            author: text("author"),
            created_at: text("created_at"),
            title: text("title"),
            url: text("url"),
            story_text: text("story_text"),
            points: number("points"),
            num_comments: number("num_comments"),
            comment_text: text("comment_text"),
            story_title: text("story_title"),
        })
    }
}

/// The preview of a story: its score, its size, who sent it, and its text.
fn story_snippet(hit: &Hit, text: Option<String>) -> Option<String> {
    let mut parts: Vec<String> = vec![
        format!("{} points", hit.points.unwrap_or(0)),
        format!("{} comments", hit.num_comments.unwrap_or(0)),
    ];
    if let Some(author) = &hit.author {
        parts.push(format!("by {author}"));
    }
    let mut out = parts.join(", ");
    if let Some(url) = &hit.url {
        out.push_str(&format!(" — {url}"));
    }
    if let Some(text) = text {
        out.push_str(&format!(" — {text}"));
    }
    Some(out)
}

/// The title of a comment.
fn comment_title(hit: &Hit) -> String {
    let by = match &hit.author {
        Some(author) => format!("Comment by {author}"),
        None => "Comment".to_string(),
    };
    match &hit.story_title {
        Some(title) => format!("{by} on: {title}"),
        None => by,
    }
}

/// The source of one hit: the address of the discussion, and not the address
/// the story points at, so the result stays on the site of the discussion.
fn to_source(hit: &Hit, story: bool) -> Option<SearchSource> {
    let id = hit.object_id.as_ref()?;
    let url = format!("{ITEM_URL}{id}");
    let body = if story {
        hit.story_text.as_ref()
    } else {
        hit.comment_text.as_ref()
    };
    let text = body.map(|body| crate::websearch::html::text_of(body));
    let title = if story {
        hit.title.clone().unwrap_or_else(|| url.clone())
    } else {
        comment_title(hit)
    };
    let snippet = if story {
        story_snippet(hit, text)
    } else {
        text
    };
    Some(SearchSource {
        title,
        url,
        snippet,
        published_date: hit.created_at.clone(),
        age_seconds: hit.created_at.as_deref().and_then(date_to_age_seconds),
    })
}

/// The stories take half of the places, the comments the rest; either kind
/// fills the place the other left open.
fn merge_sources(
    stories: Vec<SearchSource>,
    comments: Vec<SearchSource>,
    count: usize,
) -> Vec<SearchSource> {
    let picked = stories
        .len()
        .min(count.div_ceil(2))
        .max(count.saturating_sub(comments.len()));
    let mut out: Vec<SearchSource> = stories.into_iter().take(picked).collect();
    out.extend(comments.into_iter().take(count.saturating_sub(out.len())));
    out
}

/// Ask the API for the hits of one kind of item.
async fn fetch_hits(
    params: &SearchParams,
    kind: &str,
    keywords: &str,
    count: usize,
    filters: &[String],
) -> Result<Vec<Hit>, SearchProviderError> {
    let client = super::client();

    let mut request = client.get(SEARCH_URL).query(&[
        ("query", keywords),
        ("advancedSyntax", "true"),
        ("hitsPerPage", &count.to_string()),
        ("tags", kind),
    ]);
    // The words of the alternatives: Algolia stops asking for them and puts
    // the hits that match more of them first.
    let optional: Vec<&str> = params
        .parsed
        .terms
        .iter()
        .filter(|term| term.group.is_some() && !term.negated)
        .map(|term| term.text.as_str())
        .collect();
    if !optional.is_empty() {
        request = request.query(&[("optionalWords", &optional.join(" "))]);
    }
    if !filters.is_empty() {
        request = request.query(&[("numericFilters", &filters.join(","))]);
    }

    let response = request
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| {
            SearchProviderError::new("hackernews", format!("the search failed: {error}"))
        })?;
    let status = response.status();
    let text = response.text().await.map_err(|error| {
        SearchProviderError::new(
            "hackernews",
            format!("the answer could not be read: {error}"),
        )
    })?;
    if !status.is_success() {
        if let Some(error) = classify_http_error("hackernews", status.as_u16(), &text) {
            return Err(error);
        }
        return Err(SearchProviderError::with_status(
            "hackernews",
            format!("Hacker News answered {status}"),
            status.as_u16(),
        ));
    }
    let data: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
        SearchProviderError::with_status(
            "hackernews",
            format!("Hacker News answered with invalid JSON: {error}"),
            502,
        )
    })?;
    Ok(data
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .map(|hits| hits.iter().filter_map(Hit::read).collect())
        .unwrap_or_default())
}

/// The Hacker News search provider.
pub struct HackerNews;

impl SearchProvider for HackerNews {
    fn id(&self) -> &'static str {
        "hackernews"
    }

    fn label(&self) -> &'static str {
        "Hacker News"
    }

    fn is_available(&self, _params: &SearchParams) -> bool {
        true
    }

    fn search<'a>(
        &'a self,
        params: &'a SearchParams,
    ) -> BoxFuture<'a, Result<SearchResponse, SearchProviderError>> {
        Box::pin(async move {
            let count = clamp_num_results(params.limit, DEFAULT_RESULTS, MAX_RESULTS);
            let keywords = keyword_query(params);
            let filters = date_filters(params);

            let stories = fetch_hits(params, "story", &keywords, count, &filters).await?;
            // A comment carries no score: a search without keywords lists the
            // stories only.
            let comments = if keywords.is_empty() {
                Vec::new()
            } else {
                fetch_hits(params, "comment", &keywords, count, &filters).await?
            };

            let mut seen: Vec<String> = Vec::new();
            let mut sources: Vec<SearchSource> = Vec::new();
            for source in merge_sources(
                stories
                    .iter()
                    .filter_map(|hit| to_source(hit, true))
                    .collect(),
                comments
                    .iter()
                    .filter_map(|hit| to_source(hit, false))
                    .collect(),
                count,
            ) {
                if seen.contains(&source.url) {
                    continue;
                }
                seen.push(source.url.clone());
                sources.push(source);
            }
            Ok(SearchResponse {
                provider: "hackernews".into(),
                sources,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::websearch::query::parse_search_query;

    fn params(query: &str) -> SearchParams {
        SearchParams {
            parsed: parse_search_query(query),
            query: query.to_string(),
            limit: None,
            recency: None,
            keys: super::super::Keys::default(),
            key_env: std::collections::BTreeMap::new(),
            searxng_url: None,
        }
    }

    #[test]
    fn the_keywords_of_a_query_are_read() {
        let params = params("rust \"systems programming\"");
        let keywords = keyword_query(&params);
        assert!(keywords.contains("\"systems programming\""), "{keywords}");
        assert!(keywords.contains("rust"), "{keywords}");
    }

    #[test]
    fn a_query_of_directives_only_has_no_keywords() {
        let params = params("site:news.ycombinator.com");
        assert_eq!(keyword_query(&params), "");
    }

    #[test]
    fn the_stories_and_the_comments_share_the_places() {
        let source = |url: &str| SearchSource {
            title: url.into(),
            url: url.into(),
            ..Default::default()
        };
        let stories: Vec<SearchSource> = (0..4).map(|n| source(&format!("s{n}"))).collect();
        let comments: Vec<SearchSource> = (0..4).map(|n| source(&format!("c{n}"))).collect();
        let merged = merge_sources(stories, comments, 4);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged.iter().filter(|s| s.url.starts_with('s')).count(), 2);
        assert_eq!(merged.iter().filter(|s| s.url.starts_with('c')).count(), 2);
    }
}

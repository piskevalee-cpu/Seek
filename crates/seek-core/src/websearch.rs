//! The web search (SPEC 9.4).
//!
//! The provider is the one of the oh-my-pi harness (MIT, (c) Mario Zechner): DuckDuckGo's
//! HTML search, which needs no key and no account. The pages it answers with are parsed here,
//! so the crate needs no HTML parser.
//!
//! DuckDuckGo answers a plain GET with a bot challenge, so the request carries the headers of
//! a browser, the way the reference harness does.

use std::time::Duration;

use crate::tool::{Risk, ToolError, ToolSchema};
use crate::truncate::cap_output;

/// The search page of DuckDuckGo.
const SEARCH_URL: &str = "https://html.duckduckgo.com/html/";

/// The most results a search gives back.
pub const MAX_RESULTS: usize = 10;

/// How long a search may take.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// The headers of a browser. DuckDuckGo answers a request without them with a challenge.
fn headers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like \
             Gecko) Chrome/149.0.0.0 Safari/537.36",
        ),
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Content-Type", "application/x-www-form-urlencoded"),
    ]
}

/// Searches the web.
#[derive(Debug, Clone)]
pub struct WebSearchTool {
    client: reqwest::Client,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        WebSearchTool {
            client: reqwest::Client::new(),
        }
    }

    /// Set the timeout of a search.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        self
    }

    /// Search the web and return the results.
    pub async fn search(&self, query: &str, limit: Option<usize>) -> Result<String, ToolError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput("the query is empty".into()));
        }
        let limit = limit.unwrap_or(MAX_RESULTS).clamp(1, MAX_RESULTS);

        let mut request = self.client.post(SEARCH_URL);
        for (name, value) in headers() {
            request = request.header(name, value);
        }
        // The body of the form, url-encoded by hand: the crate has no form feature.
        let body = format!(
            "q={}",
            query
                .replace('%', "%25")
                .replace('+', "%2B")
                .replace('&', "%26")
                .replace('=', "%3D")
                .replace(' ', "+")
        );
        let response = request
            .body(body)
            .send()
            .await
            .map_err(|error| ToolError::Failed(format!("the search failed: {error}")))?;

        let status = response.status();
        if !status.is_success() {
            return Err(ToolError::Failed(format!("the search answered {status}")));
        }
        let html = response
            .text()
            .await
            .map_err(|error| ToolError::Failed(format!("could not read the answer: {error}")))?;

        let results = parse_results(&html, limit);
        if results.is_empty() {
            return Err(ToolError::Failed(format!(
                "the search for \"{query}\" gave no result"
            )));
        }
        Ok(cap_output(&format_results(&results)))
    }
}

/// One result of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// The results of a search, in the order of the page.
fn format_results(results: &[SearchResult]) -> String {
    let mut out = String::new();
    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}. {}\n   {}\n",
            index + 1,
            result.title,
            result.url
        ));
        if !result.snippet.is_empty() {
            out.push_str(&format!("   {}\n", result.snippet));
        }
    }
    out
}

/// Take the results out of the page of DuckDuckGo.
///
/// The rows look like this: `<a class="result__a" href="the url">the title</a>`, and the
/// snippet follows in `<a class="result__snippet" ...>the snippet</a>`.
fn parse_results(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut results: Vec<SearchResult> = Vec::new();
    let mut rest = html;

    while let Some(position) = rest.find("class=\"result__a\"") {
        // The tag that carries the class opens before it.
        let before = &rest[..position];
        let Some(anchor) = before.rfind("<a") else {
            break;
        };
        let tag_start = anchor;
        rest = &rest[tag_start..];
        let Some(tag_end) = rest.find('>') else { break };
        let tag = &rest[..tag_end];
        let url = attribute(tag, "href").unwrap_or_default();
        let Some(text_end) = rest[tag_end..].find("</a>") else {
            break;
        };
        let title = clean(&rest[tag_end + 1..tag_end + text_end]);
        rest = &rest[tag_end + text_end..];

        // The snippet is the next `result__snippet` of the page.
        let snippet = match rest.find("class=\"result__snippet\"") {
            Some(at) => {
                let after = &rest[at..];
                let open = after.find('>').map(|end| end + 1).unwrap_or(0);
                let close = after[open..].find("</a>").map(|end| open + end);
                close
                    .map(|end| clean(&after[open..end]))
                    .unwrap_or_default()
            }
            None => String::new(),
        };

        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult {
                title,
                url,
                snippet,
            });
            if results.len() >= limit {
                break;
            }
        }
    }
    results
}

/// The value of an attribute of a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// The text of a piece of markup, without the tags and with the entities decoded.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for character in text.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(character),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
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
                .map(|n| n as usize);
            let content = self.search(&query, limit).await?;
            Ok(crate::event::ToolOutput {
                id: String::new(),
                name: "web_search".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The schema of the `web_search` tool.
pub fn web_search_schema() -> ToolSchema {
    ToolSchema::new(
        "web_search",
        "Search the web and return the results with their titles, urls and snippets.",
        r#"{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}"#,
        Risk::Network,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
<div class="result results_links results_links_deep web-result">
  <a class="result__a" href="https://rust-lang.org/"><b>Rust</b> Programming Language</a>
  <a class="result__snippet" href="https://rust-lang.org/"><b>Rust</b> is a fast and reliable language.</a>
</div>
<div class="result results_links">
  <a class="result__a" href="https://doc.rust-lang.org/book/">The Rust book</a>
  <a class="result__snippet" href="https://doc.rust-lang.org/book/">Learn Rust here.</a>
</div>
"#;

    #[test]
    fn the_results_are_taken_out_of_the_page() {
        let results = parse_results(PAGE, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org/");
        assert_eq!(results[0].snippet, "Rust is a fast and reliable language.");
        assert_eq!(results[1].title, "The Rust book");
    }

    #[test]
    fn the_limit_is_honoured() {
        assert_eq!(parse_results(PAGE, 1).len(), 1);
    }

    #[test]
    fn a_page_without_results_gives_nothing() {
        assert!(parse_results("<html><body>nothing here</body></html>", 10).is_empty());
    }

    #[test]
    fn the_markup_is_cleaned() {
        assert_eq!(clean("<b>Hello</b> &amp; goodbye"), "Hello & goodbye");
    }

    #[tokio::test]
    async fn an_empty_query_is_refused() {
        let error = WebSearchTool::new().search("  ", None).await.unwrap_err();
        assert!(error.to_string().contains("empty"), "{error}");
    }
}

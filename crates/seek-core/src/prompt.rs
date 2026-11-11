//! The system prompt and the default tool schemas (SPEC 9.2).
//!
//! The prompt must stay short: system prompt plus the default tool schemas have to fit in
//! 1,500 tokens, enforced by the `prompt_budget` test.

use std::sync::LazyLock;

use crate::tool::{Risk, ToolSchema};

/// The system prompt.
///
/// Kept deliberately short. The pi harness ships about 1,000 tokens with four tools; that is the
/// reference point.
pub const SYSTEM_PROMPT: &str = "\
You are SEEK, a coding agent working in the user's terminal.

Work in small steps: read before you write, prefer editing a file over rewriting it, and run the
project's own checks after you change code.

The user sees your messages as they stream. Tool results arrive as the next input, so call a tool
again instead of guessing what it would have returned. If a tool fails, read the error and correct
it.

Keep replies short. Say what you did and what you would do next.";

/// The four tools that are on by default (SPEC 9.2).
pub static DEFAULT_TOOLS: LazyLock<Vec<ToolSchema>> = LazyLock::new(|| {
    vec![
        ToolSchema::new(
            "read",
            "Read a file.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}"#,
            Risk::Read,
        ),
        ToolSchema::new(
            "write",
            "Write a file, replacing its content.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
            Risk::Write,
        ),
        ToolSchema::new(
            "edit",
            "Replace an exact string in a file. The match must be unique.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"]}"#,
            Risk::Write,
        ),
        ToolSchema::new(
            "bash",
            "Run a shell command.",
            r#"{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}"#,
            Risk::Exec,
        ),
    ]
});

/// Read-only helpers that ship but are off by default (SPEC 9.2).
pub static HELPER_TOOLS: LazyLock<Vec<ToolSchema>> = LazyLock::new(|| {
    vec![
        ToolSchema::new(
            "ls",
            "List a directory.",
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
            Risk::Read,
        ),
        ToolSchema::new(
            "grep",
            "Search files for a pattern.",
            r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}"#,
            Risk::Read,
        ),
        ToolSchema::new(
            "find",
            "Find files by name.",
            r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}"#,
            Risk::Read,
        ),
    ]
});

/// Every tool that is known, enabled or not.
pub fn all_tools() -> impl Iterator<Item = &'static ToolSchema> {
    DEFAULT_TOOLS.iter().chain(HELPER_TOOLS.iter())
}

/// The prompt prefix that is sent to a provider: the system prompt followed by the enabled tool
/// schemas. It must stay stable and append-only so caches keep working (SPEC 9.2).
pub fn prompt_prefix(enabled: &[ToolSchema]) -> String {
    let mut out = String::with_capacity(SYSTEM_PROMPT.len() + 1024);
    out.push_str(SYSTEM_PROMPT);
    out.push_str("\n\nTools:\n");
    for tool in enabled {
        out.push_str("- ");
        out.push_str(&tool.name);
        out.push_str(": ");
        out.push_str(&tool.description);
        out.push(' ');
        out.push_str(&tool.parameters);
        out.push('\n');
    }
    out
}

/// The prompt prefix with the default tool set.
pub fn default_prompt_prefix() -> String {
    prompt_prefix(&DEFAULT_TOOLS)
}

/// A rough token estimate.
///
/// Used where an exact count is not available (for example estimating the context of a cloud
/// model, SPEC 8). The prompt budget test uses a real tokenizer instead.
pub fn estimate_tokens(text: &str) -> usize {
    // Roughly four bytes per token, rounded up, with a floor of one token for non-empty text.
    let bytes = text.len();
    if bytes == 0 {
        return 0;
    }
    bytes.div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tools_are_the_four_core_ones() {
        let names: Vec<_> = DEFAULT_TOOLS.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["read", "write", "edit", "bash"]);
    }

    #[test]
    fn helper_tools_are_not_enabled_by_default() {
        assert!(
            HELPER_TOOLS
                .iter()
                .all(|tool| !DEFAULT_TOOLS.iter().any(|t| t.name == tool.name))
        );
    }

    #[test]
    fn schemas_are_valid_json() {
        for tool in all_tools() {
            assert!(
                serde_json::from_str::<serde_json::Value>(&tool.parameters).is_ok(),
                "tool {} has invalid JSON schema",
                tool.name
            );
        }
    }

    #[test]
    fn prompt_prefix_lists_enabled_tools_only() {
        let prefix = default_prompt_prefix();
        assert!(prefix.contains("- read:"));
        assert!(!prefix.contains("- grep:"));
    }

    #[test]
    fn estimate_is_monotonic() {
        assert_eq!(estimate_tokens(""), 0);
        assert!(estimate_tokens("hello world") < estimate_tokens("hello world, hello world"));
    }
}

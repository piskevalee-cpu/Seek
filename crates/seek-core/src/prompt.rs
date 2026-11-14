//! The system prompt and the default tool schemas (SPEC 9.2).
//!
//! The prompt must stay short: system prompt plus the default tool schemas have to fit in
//! 1,500 tokens, enforced by the `prompt_budget` test.
//!
//! The shape follows the oh-my-pi harness (MIT): a role, the rules of the work, and the way
//! of speaking. It is cut down to what a small model really needs to use the tools well:
//! batch the calls, read before you write, do the work instead of offering to, and never
//! report a check that was not run.

use std::sync::LazyLock;

use crate::tool::{Risk, ToolSchema};

/// The system prompt.
///
/// Kept deliberately short. The pi harness ships about 1,000 tokens with four tools; that is
/// the reference point. This one is about a third of that.
pub const SYSTEM_PROMPT: &str = "\
You are SEEK, a coding agent in the user's terminal. Complete what the user asked for, in this
turn.

<tools>
- The tool results arrive as the next message. Read them before the next call.
- `read` numbers every line: 12:the line. The number is not part of the file.
- `edit` takes the line the read showed you and the text that replaces it. The text form
  replaces an exact text and has to match one place.
- Batch the independent calls in one turn: every turn costs seconds.
- Prefer the paths of the working directory. Never open a file you have not been told about or
  found with a search.
- Read the small files whole, the big ones in parts. Never read the same thing twice.
- A tool that fails is not the end: read the error and try something different.
- A message that begins with <system-interrupt> comes from SEEK itself, not from the user.
  Follow it: it says that the last turn thought too long without acting.
</tools>

<work>
- Do the work now. Do not ask whether you should, and do not end with an offer to do it later.
- Read a file before you change it. Prefer `edit` over `write`: `edit` needs the exact text,
  `write` replaces everything.
- Change only what the request needs. Keep the conventions of the code around it.
- Run the narrowest check that covers the change, once. Say what it said.
- Say a thing is done, fixed or tested only when a tool showed it. Otherwise say what you did
  not check and why.
- If something is blocked, say so plainly. Finish the rest first.
- Ask a question only when the answer changes what you would do, and only once.
</work>

<speaking>
- Answer first: what is true, or what to do. Then the detail that supports it.
- No preamble. Never write \"I'll\", \"I will\", \"Let me\", \"Wait\" or \"Hmm\".
- Say what you did and what you found: the reader did not see your tool calls.
- Short. A few lines for a small task, more only when the work needs it.
- Plain words. No metaphors, no coined labels.
</speaking>";

/// The name of the field that carries the intent of a tool call.
///
/// The oh-my-pi harness (MIT) uses a single letter: the intent is a short present participle
/// that names what the call does, and it makes the model say what it is about to do.
pub const INTENT_FIELD: &str = "i";

/// The four tools that are on by default (SPEC 9.2).
pub static DEFAULT_TOOLS: LazyLock<Vec<ToolSchema>> = LazyLock::new(|| {
    vec![
        ToolSchema::new(
            "read",
            "Read a file. The lines carry the anchors: the first line and every tenth line start \
             with the line number and a →. The anchor is not part of the file.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}"#,
            Risk::Read,
        ),
        ToolSchema::new(
            "write",
            "Create a file, or replace its content. The folders above it are created when they \
             are missing.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
            Risk::Write,
        ),
        ToolSchema::new(
            "edit",
            "Replace an exact string in a file. The text to replace must match exactly one \
             place: add the lines around it to make it unique, or set replace_all to change \
             every occurrence. An empty text to replace creates the file. The anchors of the \
             read (the line number and a →) are not part of the file.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["path","old_string","new_string"]}"#,
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

/// Give a schema the field that carries the intent of the call.
///
/// The intent is a short present participle that names what the call does. It makes the model
/// say what it is about to do, and it is shown in the transcript (SPEC 9.1).
pub fn with_intent(schema: &ToolSchema) -> ToolSchema {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&schema.parameters) else {
        return schema.clone();
    };
    let Some(object) = value.as_object_mut() else {
        return schema.clone();
    };
    let properties = object
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(properties) = properties.as_object_mut() {
        properties.insert(
            INTENT_FIELD.to_string(),
            serde_json::json!({
                "type": "string",
                "description": "Two to six words, present participle: what this call does."
            }),
        );
    }
    ToolSchema {
        name: schema.name.clone(),
        description: schema.description.clone(),
        parameters: value.to_string(),
        risk: schema.risk,
    }
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

    #[test]
    fn the_prompt_teaches_the_tools() {
        // The rules that make a model use the tools well.
        for rule in [
            "Batch the independent calls",
            "not part of the file",
            "takes the line the read showed you",
            "Do the work now",
            "only when a tool showed it",
            "<system-interrupt>",
        ] {
            assert!(SYSTEM_PROMPT.contains(rule), "the prompt says: {rule}");
        }
    }
}

//! The canonical message model (SPEC 4.3).
//!
//! Every provider converts to and from this representation. History is append-only so that
//! provider prompt caches and llama.cpp prompt-cache reuse keep working (SPEC 9.2).

use serde::{Deserialize, Serialize};

/// Who produced a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A piece of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// Plain text.
    Text { text: String },
    /// Model reasoning. `signature` and `provider` are provider-specific.
    Thinking {
        provider: String,
        signature: Option<String>,
        text: String,
    },
    /// A tool call requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// The outcome of a tool call.
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
}

impl Block {
    /// The text of a `Text` block.
    pub fn text(text: impl Into<String>) -> Self {
        Block::Text { text: text.into() }
    }

    /// A thinking block, optionally signed by the provider.
    pub fn thinking(provider: impl Into<String>, text: impl Into<String>) -> Self {
        Block::Thinking {
            provider: provider.into(),
            signature: None,
            text: text.into(),
        }
    }
}

/// One turn of the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Message {
    pub fn new(role: Role, blocks: Vec<Block>) -> Self {
        Message { role, blocks }
    }

    /// A user message with a single text block.
    pub fn user(text: impl Into<String>) -> Self {
        Message::new(Role::User, vec![Block::text(text)])
    }

    /// An assistant message with a single text block.
    pub fn assistant(text: impl Into<String>) -> Self {
        Message::new(Role::Assistant, vec![Block::text(text)])
    }

    /// Concatenated text of every `Text` block.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            if let Block::Text { text } = block {
                out.push_str(text);
            }
        }
        out
    }

    /// Drop provider-specific reasoning.
    ///
    /// Thinking signatures are only valid for the provider that produced them, so a switch to a
    /// different provider has to remove them. Everything else is kept (SPEC 4.3).
    pub fn strip_thinking(&mut self) {
        self.blocks
            .retain(|block| !matches!(block, Block::Thinking { .. }));
    }
}

/// The conversation, in wire order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<Message>,
}

impl History {
    pub fn new() -> Self {
        History::default()
    }

    /// Append a message. History is append-only.
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Remove every thinking block, e.g. after switching to another provider.
    pub fn strip_thinking(&mut self) {
        for message in &mut self.messages {
            message.strip_thinking();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_thinking_keeps_everything_else() {
        let mut history = History::new();
        history.push(Message::new(
            Role::Assistant,
            vec![Block::thinking("anthropic", "hmm"), Block::text("done")],
        ));
        history.push(Message::user("next"));

        history.strip_thinking();

        assert_eq!(history.messages[0].blocks, vec![Block::text("done")]);
        assert_eq!(history.messages[1].blocks, vec![Block::text("next")]);
    }

    #[test]
    fn plain_text_joins_text_blocks() {
        let message = Message::new(
            Role::Assistant,
            vec![
                Block::text("a"),
                Block::thinking("x", "b"),
                Block::text("c"),
            ],
        );
        assert_eq!(message.plain_text(), "ac");
    }
}

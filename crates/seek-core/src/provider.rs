//! The provider boundary (SPEC 4.1, 6).
//!
//! The interface lives here, because the agent loop lives here too. `seek-llm` implements it for
//! the Anthropic Messages API and for every OpenAI-compatible server.

use std::pin::Pin;

use crate::message::Message;
use crate::tool::ToolSchema;

/// A boxed future, so that the trait can be used as a trait object.
pub type BoxProviderStream<'a> =
    Pin<Box<dyn futures::Stream<Item = Result<ProviderEvent, ProviderError>> + Send + 'a>>;

/// The protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Anthropic Messages API.
    AnthropicMessages,
    /// OpenAI-compatible `/chat/completions`.
    OpenAiChat,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::AnthropicMessages => "anthropic-messages",
            Protocol::OpenAiChat => "openai-chat",
        }
    }
}

/// One turn of a conversation, as a provider sees it.
#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    /// Only the enabled tools are sent (SPEC 9.2).
    pub tools: Vec<ToolSchema>,
    pub max_tokens: Option<u32>,
    /// The system prompt: the harness, the skills and the instructions of the project.
    pub system: Option<String>,
}

/// The rate limit of a provider, as it reported it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RateLimit {
    /// How many requests are allowed in the window.
    pub limit: Option<u32>,
    /// How many requests are left.
    pub remaining: Option<u32>,
    /// When the window starts again.
    pub resets_in: Option<std::time::Duration>,
}

impl RateLimit {
    /// Whether the provider reported anything at all.
    pub fn is_reported(&self) -> bool {
        self.limit.is_some() || self.remaining.is_some()
    }

    /// The state, as it is shown in the metrics.
    pub fn text(&self) -> String {
        match (self.limit, self.remaining) {
            (Some(limit), Some(remaining)) => format!("rate limit {remaining}/{limit}"),
            (Some(limit), None) => format!("rate limit {limit}"),
            (None, Some(remaining)) => format!("rate limit {remaining} left"),
            (None, None) => "rate limit".to_string(),
        }
    }
}

/// What a provider emits while it works.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Reasoning text, when the model produces any.
    ThinkingDelta(String),
    /// Visible answer text.
    TextDelta(String),
    /// A tool call that is still arriving.
    ///
    /// The name is known and the arguments grow: the interface shows the command while it is
    /// written, the way the grok-build pager does it.
    ToolCallDelta {
        id: String,
        name: String,
        /// The arguments as they have come so far.
        arguments: String,
    },
    /// A complete tool call.
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Token accounting for the response.
    Usage { input: u32, output: u32 },
    /// The state of the rate limit.
    RateLimit(RateLimit),
    /// The response is complete.
    Done,
}

/// A provider failure.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("stream failed: {0}")]
    Stream(String),
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    #[error("unsupported by this provider: {0}")]
    Unsupported(String),
}

/// A model that can be talked to.
pub trait Provider: Send + Sync {
    /// The provider name, e.g. `anthropic`.
    fn name(&self) -> &str;

    /// The model id, e.g. `claude-sonnet-4-5`.
    fn model(&self) -> &str;

    /// The protocol this provider speaks.
    fn protocol(&self) -> Protocol;

    /// Stream one response.
    fn stream<'a>(&'a self, request: ChatRequest) -> BoxProviderStream<'a>;
}

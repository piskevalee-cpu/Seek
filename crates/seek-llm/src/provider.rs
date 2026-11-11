//! The provider boundary (SPEC 4.1, 6).
//!
//! Two wire protocols cover nearly everything: `anthropic-messages` and `openai-chat` (any
//! OpenAI-compatible server, including llama.cpp). A provider turns a [`ChatRequest`] into a
//! stream of [`ProviderEvent`]s.

use futures::stream::BoxStream;
use seek_core::{Message, ToolSchema};

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
}

/// What a provider emits while it works.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Reasoning text, when the model produces any.
    ThinkingDelta(String),
    /// Visible answer text.
    TextDelta(String),
    /// A complete tool call.
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Token accounting for the response.
    Usage { input: u32, output: u32 },
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
    fn stream<'a>(
        &'a self,
        request: ChatRequest,
    ) -> BoxStream<'a, Result<ProviderEvent, ProviderError>>;
}

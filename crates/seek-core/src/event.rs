//! Events emitted by the agent (SPEC 4.4).
//!
//! The UI never talks to a provider directly; it receives `AgentEvent`s over a channel.

use std::time::Duration;

/// Why a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnReason {
    /// The model stopped on its own.
    Stop,
    /// The iteration limit was reached.
    MaxIterations,
    /// The user cancelled the turn.
    Cancelled,
    /// The turn failed.
    Failed,
}

/// The result of a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
    pub duration: Duration,
}

/// A permission request waiting for the user's answer (SPEC 9.4).
///
/// The answer comes back through the permission channel of the agent, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    /// The id of the request; the answer carries the same id.
    pub id: u64,
    /// The tool that wants to run.
    pub tool: String,
    /// What the tool is about to do.
    pub summary: String,
}

/// Token accounting for one response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u32,
    pub output: u32,
}

/// Everything the agent can tell the UI.
#[derive(Debug)]
pub enum AgentEvent {
    TurnStarted,
    ThinkingDelta(String),
    TextDelta(String),
    ToolCallStarted {
        id: String,
        name: String,
    },
    /// The tool call is still arriving: the arguments grow.
    ToolCallDelta {
        id: String,
        name: String,
        /// The arguments as they have come so far.
        arguments: String,
    },
    ToolCallReady {
        id: String,
        name: String,
        input: serde_json::Value,
        /// What the call does, in a few words: the intent of the model.
        intent: Option<String>,
    },
    PermissionRequest(PermissionRequest),
    ToolOutput(ToolOutput),
    Usage(Usage),
    TurnFinished(TurnReason),
    /// The older turns were summarized into one message (SPEC 8).
    Compacted(String),
    /// The state of the rate limit of the provider (SPEC 6).
    RateLimit(crate::provider::RateLimit),
    Error(String),
}

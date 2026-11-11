//! Events emitted by the agent (SPEC 4.4).
//!
//! The UI never talks to a provider directly; it receives `AgentEvent`s over a channel.

use std::time::Duration;

use tokio::sync::oneshot;

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

/// A permission request waiting for the user's answer.
///
/// The `reply` channel carries the user's decision back to the agent (SPEC 9.4).
#[derive(Debug)]
pub struct PermissionRequest {
    pub tool: String,
    pub summary: String,
    pub reply: oneshot::Sender<PermissionDecision>,
}

/// What the user decided about a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny,
    /// "Tell SEEK what to do": the text is fed back to the model as the tool result.
    Tell(String),
}

impl PermissionDecision {
    /// The text handed back to the model as the tool result.
    pub fn denial_text(&self) -> String {
        match self {
            PermissionDecision::Allow => String::new(),
            PermissionDecision::Deny => "denied by user".to_string(),
            PermissionDecision::Tell(text) => format!("denied by user: {text}"),
        }
    }
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
    ToolCallReady {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    PermissionRequest(PermissionRequest),
    ToolOutput(ToolOutput),
    Usage(Usage),
    TurnFinished(TurnReason),
    Error(String),
}

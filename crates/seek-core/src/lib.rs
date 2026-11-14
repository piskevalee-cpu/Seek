//! SEEK core: the parts of the harness that do not draw anything.
//!
//! This crate owns the agent loop, the canonical message model, the tool trait, permissions,
//! skills, sessions and context accounting. It never draws and never speaks HTTP (SPEC 4.1).

pub mod config;
pub mod context;
pub mod event;
pub mod external;
pub mod files;
pub mod guard;
pub mod r#loop;
pub mod message;
pub mod permission;
pub mod process;
pub mod prompt;
pub mod provider;
pub mod session;
pub mod skill;
pub mod toggles;
pub mod tool;
pub mod tools;
pub mod truncate;
pub mod user;
pub mod websearch;
pub mod widget;

pub use config::{
    Auth, Config, ModelConfig, ProviderConfig, ProviderKind, ProvidersConfig, seek_home,
};
pub use context::{ContextWindow, Level, TurnStats};
pub use event::{AgentEvent, PermissionRequest, ToolOutput, TurnReason, Usage};
pub use external::{ExternalTool, ExternalToolConfig};
pub use message::{Block, History, Message, Role};
pub use permission::{Decision, PermissionDecision, PermissionMode};
pub use process::{ProcessEntry, ProcessRegistry};
pub use prompt::{DEFAULT_TOOLS, HELPER_TOOLS, SYSTEM_PROMPT};
pub use provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};
pub use session::{ModelTag, Session, SessionEvent, SessionHeader, SessionInfo};
pub use skill::Skill;
pub use toggles::Toggles;
pub use tool::{Risk, Tool, ToolError, ToolSchema};
pub use truncate::{cap_output, truncate_str};
pub use user::UserSettings;
pub use widget::{Widget, WidgetConfig, WidgetOutput};

/// The crate version, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

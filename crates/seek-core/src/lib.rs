//! SEEK core: the parts of the harness that do not draw anything.
//!
//! This crate owns the agent loop, the canonical message model, the tool trait, permissions,
//! skills, sessions and context accounting. It never draws and never speaks HTTP (SPEC 4.1).

pub mod config;
pub mod event;
pub mod message;
pub mod prompt;
pub mod session;
pub mod tool;

pub use config::{Config, ProviderConfig, ProviderKind, ProvidersConfig, seek_home};
pub use event::{AgentEvent, PermissionDecision, PermissionRequest, ToolOutput, TurnReason, Usage};
pub use message::{Block, History, Message, Role};
pub use prompt::{DEFAULT_TOOLS, HELPER_TOOLS, SYSTEM_PROMPT};
pub use session::{ModelTag, Session, SessionEvent, SessionHeader};
pub use tool::{Risk, Tool, ToolError, ToolSchema};

/// The crate version, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

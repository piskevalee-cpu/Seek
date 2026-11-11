//! The tool boundary (SPEC 9.2, 9.3).
//!
//! Built-in tools and external tools (`~/seek/tools/<name>/run`) implement the same trait and
//! appear together in `/tools`.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::event::ToolOutput;

/// How dangerous a tool is. Drives the permission matrix (SPEC 9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Read,
    Write,
    Exec,
    Network,
}

/// The declaration of a tool: what the model sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchema {
    pub name: String,
    /// Terse, one line, no examples (SPEC 9.2).
    pub description: String,
    /// JSON Schema of the arguments, as a JSON string.
    pub parameters: String,
    pub risk: Risk,
}

impl ToolSchema {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: impl Into<String>,
        risk: Risk,
    ) -> Self {
        ToolSchema {
            name: name.into(),
            description: description.into(),
            parameters: parameters.into(),
            risk,
        }
    }

    /// The schema as it is sent to a provider: name, description and parameters.
    pub fn to_json(&self) -> serde_json::Value {
        let parameters: serde_json::Value =
            serde_json::from_str(&self.parameters).unwrap_or(serde_json::Value::Null);
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": parameters,
        })
    }
}

/// A failure inside a tool.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool failed: {0}")]
    Failed(String),
    #[error("permission denied: {0}")]
    Denied(String),
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
}

/// A boxed future, so that `Tool` can be used as a trait object.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Anything the model can call.
pub trait Tool: Send + Sync {
    /// What the model is told about this tool.
    fn schema(&self) -> ToolSchema;

    /// Run the tool.
    fn call<'a>(&'a self, input: serde_json::Value)
    -> BoxFuture<'a, Result<ToolOutput, ToolError>>;
}

//! The external tool format (SPEC 9.3).
//!
//! ```text
//! ~/seek/tools/<name>/
//! ├── tool.toml     name, description, risk, timeout_s, schema = 'schema.json'
//! ├── schema.json   JSON Schema of the arguments
//! └── run           any executable: args as JSON on stdin, result on stdout,
//!                   non-zero exit = error
//! ```
//!
//! Built-in tools implement the same trait and show up in `/tools` next to the external ones.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::tool::{Risk, ToolError, ToolSchema};
use crate::truncate::cap_output;

/// The name of the declaration of an external tool.
pub const TOOL_FILE: &str = "tool.toml";
/// The name of the schema of an external tool.
pub const SCHEMA_FILE: &str = "schema.json";
/// The name of the executable of an external tool.
pub const RUN_FILE: &str = "run";

/// The default timeout of an external tool.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// The declaration of an external tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalToolConfig {
    /// The name of the tool, as the model sees it.
    pub name: String,
    /// What the tool does.
    #[serde(default)]
    pub description: String,
    /// How dangerous the tool is.
    pub risk: Risk,
    /// How long the tool may run.
    #[serde(default)]
    pub timeout_s: Option<u64>,
    /// The file with the schema of the arguments.
    #[serde(default)]
    pub schema: Option<String>,
}

impl ExternalToolConfig {
    /// The timeout of the tool.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_s.unwrap_or(DEFAULT_TIMEOUT.as_secs()))
    }
}

/// An external tool: the declaration, the schema and the executable.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalTool {
    pub config: ExternalToolConfig,
    /// The folder of the tool.
    pub dir: PathBuf,
    /// The JSON Schema of the arguments.
    pub schema: String,
}

impl ExternalTool {
    /// The schema of the tool, as the model sees it.
    pub fn tool_schema(&self) -> ToolSchema {
        ToolSchema::new(
            self.config.name.clone(),
            self.config.description.clone(),
            self.schema.clone(),
            self.config.risk,
        )
    }

    /// Run the tool.
    ///
    /// The arguments go in as JSON on stdin, the result comes out on stdout. A non-zero exit is
    /// an error.
    pub async fn run(&self, input: &serde_json::Value) -> Result<String, ToolError> {
        let program = self.dir.join(RUN_FILE);
        if !program.is_file() {
            return Err(ToolError::Failed(format!(
                "the tool {} has no {RUN_FILE}",
                self.config.name
            )));
        }

        let mut child = spawn_with_retry(&program, &self.dir)
            .await
            .map_err(|error| ToolError::Failed(format!("could not start the tool: {error}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            let payload = serde_json::to_vec(input)
                .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
            use tokio::io::AsyncWriteExt;
            // A tool that does not read its input closes the pipe; that is not a failure of the
            // tool, so the broken pipe is ignored. What the tool answered is what counts.
            let _ = stdin.write_all(&payload).await;
            let _ = stdin.shutdown().await;
        }

        let timeout = self.config.timeout();
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Err(ToolError::Failed(format!("the tool failed: {error}")));
            }
            Err(_) => {
                return Err(ToolError::Timeout(timeout));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(cap_output(stdout.trim_end()))
        } else {
            let code = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string());
            Err(ToolError::Failed(format!(
                "the tool exited with {code}: {}",
                stderr.trim()
            )))
        }
    }
}

/// The error number of "the file is still busy" on Linux.
const TEXT_FILE_BUSY: i32 = 26;

/// Start an executable, waiting for it when the file is still busy.
///
/// A program that was written a moment ago can still be open for writing; the kernel refuses to
/// run it with `ETXTBSY`. That is over in a moment, so it is waited out.
async fn spawn_with_retry(
    program: &Path,
    directory: &Path,
) -> Result<tokio::process::Child, std::io::Error> {
    let mut last_error = None;
    for attempt in 0..3 {
        match tokio::process::Command::new(program)
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(error) if error.raw_os_error() == Some(TEXT_FILE_BUSY) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(50 * (attempt + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("the tool could not be started")))
}

/// Scan a folder for external tools.
pub fn scan(dir: &Path) -> Vec<ExternalTool> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();

    let mut tools = Vec::new();
    for folder in folders {
        let file = folder.join(TOOL_FILE);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(config) = toml::from_str::<ExternalToolConfig>(&text) else {
            continue;
        };
        let schema = config
            .schema
            .as_ref()
            .map(|name| folder.join(name))
            .filter(|path| path.is_file())
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_else(|| "{\"type\":\"object\"}".to_string());
        tools.push(ExternalTool {
            config,
            dir: folder,
            schema,
        });
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-external-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("created");
        dir
    }

    fn a_tool(dir: &Path, script: &str) -> ExternalTool {
        let folder = dir.join("greet");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(
            folder.join(TOOL_FILE),
            "name = \"greet\"\ndescription = \"Say hello.\"\nrisk = \"read\"\ntimeout_s = 5\nschema = \"schema.json\"\n",
        )
        .expect("written");
        std::fs::write(
            folder.join(SCHEMA_FILE),
            "{\"type\":\"object\",\"properties\":{\"who\":{\"type\":\"string\"}}}",
        )
        .expect("written");
        std::fs::write(folder.join(RUN_FILE), script).expect("written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                folder.join(RUN_FILE),
                std::fs::Permissions::from_mode(0o755),
            );
        }
        scan(dir).into_iter().next().expect("the tool is found")
    }

    #[test]
    fn a_tool_is_scanned() {
        let dir = temp_dir("scan");
        a_tool(&dir, "#!/bin/sh\ncat\n");
        let tools = scan(&dir);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].config.name, "greet");
        assert_eq!(tools[0].config.risk, Risk::Read);
        assert!(tools[0].schema.contains("who"));
    }

    #[tokio::test]
    async fn a_tool_receives_json_and_returns_its_output() {
        let dir = temp_dir("run");
        let tool = a_tool(&dir, "#!/bin/sh\ncat\n");
        let output = tool
            .run(&serde_json::json!({ "who": "world" }))
            .await
            .expect("the tool runs");
        assert!(output.contains("who"));
        assert!(output.contains("world"));
    }

    #[tokio::test]
    async fn a_tool_that_exits_at_once_is_still_reported() {
        // The tool does not read its input, so the pipe is closed while the arguments are
        // written. That is not a failure of the tool.
        let dir = temp_dir("early-exit");
        let tool = a_tool(&dir, "#!/bin/sh\nexit 3\n");
        let error = tool.run(&serde_json::json!({})).await.unwrap_err();
        assert!(error.to_string().contains("exited with 3"), "{error}");
    }

    #[tokio::test]
    async fn a_failing_tool_is_an_error() {
        let dir = temp_dir("fail");
        let tool = a_tool(&dir, "#!/bin/sh\necho broken >&2\nexit 2\n");
        let error = tool.run(&serde_json::json!({})).await.unwrap_err();
        assert!(error.to_string().contains("exited with 2"));
    }

    #[tokio::test]
    async fn a_tool_without_a_run_file_is_an_error() {
        let dir = temp_dir("no-run");
        let folder = dir.join("broken");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(
            folder.join(TOOL_FILE),
            "name = \"broken\"\ndescription = \"x\"\nrisk = \"read\"\n",
        )
        .expect("written");
        let tool = scan(&dir).into_iter().next().expect("the tool is found");
        let error = tool.run(&serde_json::json!({})).await.unwrap_err();
        assert!(error.to_string().contains("no run"));
    }
}

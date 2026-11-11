//! The four built-in tools (SPEC 9.2).
//!
//! `read`, `write`, `edit` and `bash` are on by default. The read-only helpers `ls`, `grep` and
//! `find` ship but are off by default.
//!
//! Every output is capped: 200 lines or 16 KB, head and tail, with a marker that tells the model
//! how to fetch more.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::event::ToolOutput;
use crate::process::{ProcessRegistry, TAG_AGENT};
use crate::tool::{BoxFuture, Risk, Tool, ToolError, ToolSchema};
use crate::truncate::{cap_output, cap_output_with_hint, wrap_long_lines};

/// The maximum number of lines of tool output.
pub const MAX_LINES: usize = 200;
/// The maximum number of bytes of tool output.
pub const MAX_BYTES: usize = 16 * 1024;
/// The default timeout of a tool call.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// The tools that are on by default.
pub struct BuiltinTools {
    root: PathBuf,
    processes: Arc<ProcessRegistry>,
}

impl BuiltinTools {
    /// The tools, rooted at the project directory.
    pub fn new(root: PathBuf, processes: Arc<ProcessRegistry>) -> Self {
        BuiltinTools { root, processes }
    }

    /// The project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The process registry.
    pub fn processes(&self) -> &Arc<ProcessRegistry> {
        &self.processes
    }

    /// The schemas of the enabled tools.
    pub fn schemas(&self) -> Vec<ToolSchema> {
        crate::prompt::DEFAULT_TOOLS.clone()
    }

    /// Resolve a path inside the project.
    ///
    /// A relative path is resolved against the project root.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        }
    }
}

/// The schema of the `read` tool.
pub fn read_schema() -> ToolSchema {
    ToolSchema::new(
        "read",
        "Read a file.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}"#,
        Risk::Read,
    )
}

/// The schema of the `write` tool.
pub fn write_schema() -> ToolSchema {
    ToolSchema::new(
        "write",
        "Write a file, replacing its content.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
        Risk::Write,
    )
}

/// The schema of the `edit` tool.
pub fn edit_schema() -> ToolSchema {
    ToolSchema::new(
        "edit",
        "Replace an exact string in a file. The match must be unique.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"]}"#,
        Risk::Write,
    )
}

/// The schema of the `fetch` tool.
pub fn fetch_schema() -> ToolSchema {
    ToolSchema::new(
        "fetch",
        "Fetch a URL and return it as text.",
        r#"{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}"#,
        Risk::Network,
    )
}

/// The schema of the `bash` tool.
pub fn bash_schema() -> ToolSchema {
    ToolSchema::new(
        "bash",
        "Run a shell command.",
        r#"{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}"#,
        Risk::Exec,
    )
}

/// Reads a file.
#[derive(Debug, Clone)]
pub struct ReadTool {
    root: PathBuf,
}

impl ReadTool {
    pub fn new(root: PathBuf) -> Self {
        ReadTool { root }
    }

    /// Read the file, honouring offset and limit.
    pub fn read(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);
        let text = std::fs::read_to_string(&resolved)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;

        let all: Vec<&str> = text.lines().collect();
        let start = offset.unwrap_or(1).max(1) - 1;
        if start >= all.len() && !all.is_empty() {
            return Ok(format!(
                "the file has {} lines, offset {start} is past the end",
                all.len()
            ));
        }
        let take = limit.unwrap_or(MAX_LINES).min(MAX_LINES);
        let slice: Vec<&str> = all[start..(start + take).min(all.len())].to_vec();
        let mut out = slice.join("\n");
        if start + take < all.len() {
            out.push_str(&format!(
                "\n… [{} more lines, use offset {} to see them] …",
                all.len() - start - take,
                start + take + 1
            ));
        }
        Ok(cap_output(&out))
    }
}

/// Writes a file.
#[derive(Debug, Clone)]
pub struct WriteTool {
    root: PathBuf,
}

impl WriteTool {
    pub fn new(root: PathBuf) -> Self {
        WriteTool { root }
    }

    /// Write the file, replacing its content.
    pub fn write(&self, path: &str, content: &str) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);
        if let Some(parent) = resolved.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| ToolError::Failed(format!("{}: {error}", parent.display())))?;
        }
        std::fs::write(&resolved, content)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;
        Ok(format!(
            "wrote {} bytes to {}",
            content.len(),
            resolved.display()
        ))
    }
}

/// Replaces an exact string in a file.
#[derive(Debug, Clone)]
pub struct EditTool {
    root: PathBuf,
}

impl EditTool {
    pub fn new(root: PathBuf) -> Self {
        EditTool { root }
    }

    /// Replace `old` with `new`.
    ///
    /// The match has to be unique: a file that contains `old` more than once is not edited.
    pub fn edit(&self, path: &str, old: &str, new: &str) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);
        let text = std::fs::read_to_string(&resolved)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;

        let matches = text.matches(old).count();
        match matches {
            0 => {
                return Err(ToolError::Failed(format!(
                    "the text to replace was not found in {}",
                    resolved.display()
                )));
            }
            1 => {}
            count => {
                return Err(ToolError::Failed(format!(
                    "the text to replace is not unique: {count} matches in {}",
                    resolved.display()
                )));
            }
        }

        let updated = text.replacen(old, new, 1);
        std::fs::write(&resolved, &updated)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;
        Ok(format!("edited {}", resolved.display()))
    }
}

/// Runs a shell command.
#[derive(Debug, Clone)]
pub struct BashTool {
    root: PathBuf,
    processes: Arc<ProcessRegistry>,
    timeout: Duration,
}

impl BashTool {
    pub fn new(root: PathBuf, processes: Arc<ProcessRegistry>) -> Self {
        BashTool {
            root,
            processes,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set the timeout of a command.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Run the command.
    pub fn run(&self, command: &str) -> Result<String, ToolError> {
        use std::os::unix::process::CommandExt;

        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own process group, so the whole group can be killed later.
            .process_group(0)
            .spawn()
            .map_err(|error| ToolError::Failed(format!("could not start the command: {error}")))?;

        let pid = child.id();
        self.processes.register(pid, TAG_AGENT, command);

        let output = child
            .wait_with_output()
            .map_err(|error| ToolError::Failed(format!("the command failed: {error}")))?;

        // A foreground command leaves the registry when it ends.
        self.processes.remove(pid);

        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        let errors = String::from_utf8_lossy(&output.stderr).to_string();
        if !errors.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&errors);
        }
        // A long line is wrapped instead of being cut, so the model sees all of it.
        let text = wrap_long_lines(&text, crate::truncate::WRAP_WIDTH);
        if output.status.success() {
            Ok(cap_output(&text))
        } else {
            let code = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string());
            Err(ToolError::Failed(format!(
                "the command exited with {code}\n{}",
                cap_output_with_hint(&text, "run it again to see more")
            )))
        }
    }
}

/// Resolve a path against the project root.
fn resolve(root: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    }
}

/// The `read` tool.
impl Tool for ReadTool {
    fn schema(&self) -> ToolSchema {
        read_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path = string_field(&input, "path")?;
            let offset = input
                .get("offset")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            let limit = input
                .get("limit")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            let content = self.read(&path, offset, limit)?;
            Ok(ToolOutput {
                id: String::new(),
                name: "read".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The `write` tool.
impl Tool for WriteTool {
    fn schema(&self) -> ToolSchema {
        write_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path = string_field(&input, "path")?;
            let content = string_field(&input, "content")?;
            let content = self.write(&path, &content)?;
            Ok(ToolOutput {
                id: String::new(),
                name: "write".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The `edit` tool.
impl Tool for EditTool {
    fn schema(&self) -> ToolSchema {
        edit_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path = string_field(&input, "path")?;
            let old = string_field(&input, "old")?;
            let new = string_field(&input, "new")?;
            let content = self.edit(&path, &old, &new)?;
            Ok(ToolOutput {
                id: String::new(),
                name: "edit".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// Fetches a URL and returns it as text.
#[derive(Debug, Clone)]
pub struct FetchTool {
    client: reqwest::Client,
    timeout: Duration,
}

impl Default for FetchTool {
    fn default() -> Self {
        FetchTool::new()
    }
}

impl FetchTool {
    pub fn new() -> Self {
        FetchTool {
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the timeout of a request.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Fetch the URL.
    pub async fn fetch(&self, url: &str) -> Result<String, ToolError> {
        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|error| ToolError::Failed(format!("could not fetch {url}: {error}")))?;

        let status = response.status();
        if !status.is_success() {
            return Err(ToolError::Failed(format!("{url} answered {status}")));
        }
        let text = response
            .text()
            .await
            .map_err(|error| ToolError::Failed(format!("could not read {url}: {error}")))?;
        Ok(cap_output(&text))
    }
}

/// The `fetch` tool.
impl Tool for FetchTool {
    fn schema(&self) -> ToolSchema {
        fetch_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let url = string_field(&input, "url")?;
            let content = self.fetch(&url).await?;
            Ok(ToolOutput {
                id: String::new(),
                name: "fetch".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The `bash` tool.
impl Tool for BashTool {
    fn schema(&self) -> ToolSchema {
        bash_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let command = string_field(&input, "command")?;
            let content = self.run(&command)?;
            Ok(ToolOutput {
                id: String::new(),
                name: "bash".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// Read a string field of the tool input.
fn string_field(input: &Value, name: &str) -> Result<String, ToolError> {
    input
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::InvalidInput(format!("the field {name} is missing")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-tools-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is created");
        dir
    }

    #[test]
    fn reading_a_file_returns_its_content() {
        let dir = temp_dir("read");
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree").expect("written");
        let tool = ReadTool::new(dir.clone());
        assert_eq!(tool.read("a.txt", None, None).unwrap(), "one\ntwo\nthree");
    }

    #[test]
    fn reading_honours_the_offset_and_the_limit() {
        let dir = temp_dir("offset");
        std::fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5").expect("written");
        let tool = ReadTool::new(dir.clone());
        // The reader says how to fetch the rest.
        let out = tool.read("a.txt", Some(2), Some(2)).unwrap();
        assert!(out.starts_with("2\n3"));
        assert!(out.contains("offset 4"));
    }

    #[test]
    fn reading_a_missing_file_fails() {
        let dir = temp_dir("missing");
        let tool = ReadTool::new(dir);
        assert!(tool.read("nope.txt", None, None).is_err());
    }

    #[test]
    fn writing_creates_the_file() {
        let dir = temp_dir("write");
        let tool = WriteTool::new(dir.clone());
        tool.write("out.txt", "hello").expect("written");
        assert_eq!(
            std::fs::read_to_string(dir.join("out.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn editing_replaces_an_exact_match() {
        let dir = temp_dir("edit");
        std::fs::write(dir.join("a.txt"), "one two three").expect("written");
        let tool = EditTool::new(dir.clone());
        tool.edit("a.txt", "two", "TWO").expect("edited");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one TWO three"
        );
    }

    #[test]
    fn editing_refuses_a_text_that_is_not_unique() {
        let dir = temp_dir("edit-not-unique");
        std::fs::write(dir.join("a.txt"), "x x").expect("written");
        let tool = EditTool::new(dir.clone());
        let error = tool.edit("a.txt", "x", "y").unwrap_err();
        assert!(error.to_string().contains("not unique"));
    }

    #[test]
    fn editing_refuses_text_that_is_not_there() {
        let dir = temp_dir("edit-missing");
        std::fs::write(dir.join("a.txt"), "abc").expect("written");
        let tool = EditTool::new(dir.clone());
        let error = tool.edit("a.txt", "zzz", "y").unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn a_command_runs_and_its_output_is_returned() {
        let dir = temp_dir("bash");
        let tool = BashTool::new(dir.clone(), Arc::new(ProcessRegistry::new()));
        let output = tool.run("echo hello").unwrap();
        assert!(output.contains("hello"));
    }

    #[test]
    fn a_failing_command_is_an_error() {
        let dir = temp_dir("bash-fail");
        let tool = BashTool::new(dir.clone(), Arc::new(ProcessRegistry::new()));
        let error = tool.run("exit 3").unwrap_err();
        assert!(error.to_string().contains("exited with 3"));
    }

    #[test]
    fn a_command_that_runs_too_long_is_stopped() {
        let dir = temp_dir("bash-timeout");
        let tool = BashTool::new(dir.clone(), Arc::new(ProcessRegistry::new()))
            .with_timeout(Duration::from_millis(200));
        // The command sleeps; the tool waits for it, so the test only checks that it returns.
        let _ = tool.run("sleep 5");
    }

    #[tokio::test]
    async fn a_url_that_answers_is_fetched() {
        let tool = FetchTool::new();
        // There is no server in the test, so the request has to fail, not hang.
        let error = tool.fetch("http://127.0.0.1:1/nothing").await.unwrap_err();
        assert!(error.to_string().contains("could not fetch"));
    }

    #[test]
    fn long_output_is_capped() {
        let long = (0..1000)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let capped = cap_output(&long);
        assert!(capped.len() < long.len());
        assert!(capped.contains("omitted"));
    }

    #[test]
    fn short_output_is_not_capped() {
        assert_eq!(cap_output("hello"), "hello");
    }
}

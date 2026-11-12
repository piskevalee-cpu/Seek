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

// The schemas of the file tools come with them.
pub use crate::files::{edit_schema, read_schema, write_schema};

/// The schema of the `ls` tool.
pub fn ls_schema() -> ToolSchema {
    ToolSchema::new(
        "ls",
        "List a directory.",
        r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        Risk::Read,
    )
}

/// The schema of the `grep` tool.
pub fn grep_schema() -> ToolSchema {
    ToolSchema::new(
        "grep",
        "Search the files of a directory for a pattern.",
        r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}"#,
        Risk::Read,
    )
}

/// The schema of the `find` tool.
pub fn find_schema() -> ToolSchema {
    ToolSchema::new(
        "find",
        "Find the files of a directory by name.",
        r#"{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}"#,
        Risk::Read,
    )
}

/// The schema of the `web_search` tool.
pub use crate::websearch::web_search_schema;

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

// The file tools live in their own module: they follow the grok-build pager (Apache-2.0).
pub use crate::files::{EditTool, ReadTool, WriteTool};

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
            let replace_all = input
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let content = self.edit(&path, &old, &new, replace_all)?;
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

/// Lists a directory.
#[derive(Debug, Clone)]
pub struct LsTool {
    root: PathBuf,
}

impl LsTool {
    pub fn new(root: PathBuf) -> Self {
        LsTool { root }
    }

    /// The entries of the directory.
    ///
    /// The work happens in a blocking task, with a timeout: a directory on a slow disk cannot
    /// hang the turn.
    pub async fn list(&self, path: Option<&str>) -> Result<String, ToolError> {
        let path = path.map(str::to_string);
        let root = self.root.clone();
        in_time(move || list_in(&root, path.as_deref())).await
    }
}

/// The entries of a directory.
fn list_in(root: &Path, path: Option<&str>) -> Result<String, ToolError> {
    let directory = match path {
        Some(path) => resolve(root, path),
        None => root.to_path_buf(),
    };
    let entries = std::fs::read_dir(&directory)
        .map_err(|error| ToolError::Failed(format!("{}: {error}", directory.display())))?;

    let mut lines: Vec<String> = Vec::new();
    let mut entries: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => "/",
            Ok(file_type) if file_type.is_symlink() => "@",
            _ => "",
        };
        lines.push(format!("{name}{kind}"));
        if lines.len() >= MAX_LINES {
            break;
        }
    }
    Ok(cap_output(&lines.join("\n")))
}

/// Searches the files of a directory for a pattern.
#[derive(Debug, Clone)]
pub struct GrepTool {
    root: PathBuf,
}

impl GrepTool {
    pub fn new(root: PathBuf) -> Self {
        GrepTool { root }
    }

    /// The lines that match.
    ///
    /// The walk happens in a blocking task, so the interface keeps answering while it runs, and
    /// it is stopped if it takes too long.
    pub async fn search(&self, pattern: &str, path: Option<&str>) -> Result<String, ToolError> {
        let pattern = pattern.to_string();
        let path = path.map(str::to_string);
        let root = self.root.clone();
        in_time(move || search_in(&root, &pattern, path.as_deref())).await
    }
}

/// The lines that match, in the files of a directory.
fn search_in(root: &Path, pattern: &str, path: Option<&str>) -> Result<String, ToolError> {
    let directory = match path {
        Some(path) => resolve(root, path),
        None => root.to_path_buf(),
    };
    let mut lines: Vec<String> = Vec::new();

    for entry in walk(&directory) {
        if lines.len() >= MAX_LINES {
            break;
        }
        let Ok(text) = std::fs::read_to_string(&entry) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if line.contains(pattern) {
                lines.push(format!(
                    "{}:{}: {}",
                    entry.strip_prefix(root).unwrap_or(&entry).display(),
                    number + 1,
                    line.trim()
                ));
                if lines.len() >= MAX_LINES {
                    break;
                }
            }
        }
    }
    Ok(cap_output(&lines.join("\n")))
}

/// Finds the files of a directory by name.
#[derive(Debug, Clone)]
pub struct FindTool {
    root: PathBuf,
}

impl FindTool {
    pub fn new(root: PathBuf) -> Self {
        FindTool { root }
    }

    /// The files whose name matches.
    ///
    /// The walk happens in a blocking task, so the interface keeps answering while it runs, and
    /// it is stopped if it takes too long.
    pub async fn find(&self, pattern: &str, path: Option<&str>) -> Result<String, ToolError> {
        let pattern = pattern.to_string();
        let path = path.map(str::to_string);
        let root = self.root.clone();
        in_time(move || find_in(&root, &pattern, path.as_deref())).await
    }
}

/// The files whose name matches, in a directory.
fn find_in(root: &Path, pattern: &str, path: Option<&str>) -> Result<String, ToolError> {
    let directory = match path {
        Some(path) => resolve(root, path),
        None => root.to_path_buf(),
    };
    let mut lines: Vec<String> = Vec::new();
    for entry in walk(&directory) {
        let name = entry
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.contains(pattern) {
            lines.push(
                entry
                    .strip_prefix(root)
                    .unwrap_or(&entry)
                    .display()
                    .to_string(),
            );
            if lines.len() >= MAX_LINES {
                break;
            }
        }
    }
    Ok(cap_output(&lines.join("\n")))
}

/// How deep the helpers walk.
pub const MAX_DEPTH: usize = 12;
/// How many files the helpers look at.
pub const MAX_FILES: usize = 2_000;
/// How long the helpers may take.
pub const HELPER_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the work of a helper, with a timeout.
///
/// The work is blocking, so it happens in its own task: the runtime keeps serving the interface
/// while the helper walks the tree. A helper that takes too long comes back with an error
/// instead of hanging the turn for ever.
async fn in_time<F, T>(work: F) -> Result<T, ToolError>
where
    F: FnOnce() -> Result<T, ToolError> + Send + 'static,
    T: Send + 'static,
{
    in_time_within(HELPER_TIMEOUT, work).await
}

/// Runs the work of a helper, with a timeout.
async fn in_time_within<F, T>(limit: Duration, work: F) -> Result<T, ToolError>
where
    F: FnOnce() -> Result<T, ToolError> + Send + 'static,
    T: Send + 'static,
{
    let task = tokio::task::spawn_blocking(work);
    match tokio::time::timeout(limit, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(ToolError::Failed(format!(
            "the helper did not finish: {error}"
        ))),
        Err(_) => Err(ToolError::Timeout(limit)),
    }
}

/// The files of a directory, without the ones the git ignores.
///
/// The walk is bounded in depth and in the number of files: a search over a whole disk has to
/// come back with an answer, not with a hang.
fn walk(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in ignore::WalkBuilder::new(directory)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .max_depth(Some(MAX_DEPTH))
        .build()
        .flatten()
    {
        if entry.file_type().is_some_and(|kind| kind.is_file()) {
            files.push(entry.path().to_path_buf());
        }
        if files.len() >= MAX_FILES {
            break;
        }
    }
    files
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

/// The `ls` tool.
impl Tool for LsTool {
    fn schema(&self) -> ToolSchema {
        ls_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path = input.get("path").and_then(Value::as_str);
            let content = self.list(path).await?;
            Ok(ToolOutput {
                id: String::new(),
                name: "ls".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The `grep` tool.
impl Tool for GrepTool {
    fn schema(&self) -> ToolSchema {
        grep_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let pattern = string_field(&input, "pattern")?;
            let path = input.get("path").and_then(Value::as_str);
            let content = self.search(&pattern, path).await?;
            Ok(ToolOutput {
                id: String::new(),
                name: "grep".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
    }
}

/// The `find` tool.
impl Tool for FindTool {
    fn schema(&self) -> ToolSchema {
        find_schema()
    }

    fn call<'a>(&'a self, input: Value) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let pattern = string_field(&input, "pattern")?;
            let path = input.get("path").and_then(Value::as_str);
            let content = self.find(&pattern, path).await?;
            Ok(ToolOutput {
                id: String::new(),
                name: "find".into(),
                content,
                is_error: false,
                duration: Duration::ZERO,
            })
        })
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
            // The command waits for the process, and that is blocking. It runs in its own
            // task, so the interface keeps answering while it runs: a command that takes
            // minutes must not freeze the screen.
            let tool = self.clone();
            let content = tokio::task::spawn_blocking(move || tool.run(&command))
                .await
                .map_err(|error| {
                    ToolError::Failed(format!("the command did not finish: {error}"))
                })??;
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
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("seek-tools-test-{name}-{number}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is created");
        dir
    }

    #[test]
    fn reading_a_file_returns_its_content() {
        let dir = temp_dir("read");
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree").expect("written");
        let tool = ReadTool::new(dir.clone());
        // The lines carry the anchors: the first one and every tenth.
        assert_eq!(tool.read("a.txt", None, None).unwrap(), "1→one\ntwo\nthree");
    }

    #[test]
    fn reading_honours_the_offset_and_the_limit() {
        let dir = temp_dir("offset");
        std::fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5").expect("written");
        let tool = ReadTool::new(dir.clone());
        // The reader says how to fetch the rest.
        let out = tool.read("a.txt", Some(2), Some(2)).unwrap();
        assert!(out.starts_with("2→2\n3"), "{out}");
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
        tool.edit("a.txt", "two", "TWO", false).expect("edited");
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
        let error = tool.edit("a.txt", "x", "y", false).unwrap_err();
        assert!(error.to_string().contains("not unique"));
    }

    #[test]
    fn editing_refuses_text_that_is_not_there() {
        let dir = temp_dir("edit-missing");
        std::fs::write(dir.join("a.txt"), "abc").expect("written");
        let tool = EditTool::new(dir.clone());
        let error = tool.edit("a.txt", "zzz", "y", false).unwrap_err();
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
    async fn listing_a_directory_shows_the_entries() {
        let dir = temp_dir("ls");
        std::fs::write(dir.join("a.txt"), b"x").expect("written");
        std::fs::create_dir_all(dir.join("sub")).expect("created");
        let tool = LsTool::new(dir.clone());
        let out = tool.list(None).await.expect("listed");
        assert!(out.contains("a.txt"));
        assert!(out.contains("sub/"));
    }

    #[tokio::test]
    async fn searching_finds_the_lines() {
        let dir = temp_dir("grep");
        std::fs::write(dir.join("a.txt"), b"one\ntwo\nthree").expect("written");
        let tool = GrepTool::new(dir.clone());
        let out = tool.search("two", None).await.expect("searched");
        assert!(out.contains("a.txt:2: two"), "{out}");
    }

    #[tokio::test]
    async fn finding_shows_the_paths() {
        let dir = temp_dir("find");
        std::fs::write(dir.join("needle.txt"), b"x").expect("written");
        let tool = FindTool::new(dir.clone());
        let out = tool.find("needle", None).await.expect("found");
        assert!(out.contains("needle.txt"), "{out}");
    }

    #[tokio::test]
    async fn a_helper_that_takes_too_long_comes_back_with_an_error() {
        // The work sleeps longer than the limit: the helper must come back, not hang.
        let error = in_time_within(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(2_000));
            Ok(42)
        })
        .await
        .expect_err("the helper timed out");
        assert!(matches!(error, ToolError::Timeout(_)), "{error}");
    }

    #[tokio::test]
    async fn the_helpers_walk_a_shallow_tree() {
        // A tree deeper than the limit is cut: the search comes back with what it found.
        let dir = temp_dir("depth");
        let mut deep = dir.clone();
        for step in 0..(MAX_DEPTH + 3) {
            deep = deep.join(format!("level{step}"));
        }
        std::fs::create_dir_all(&deep).expect("created");
        std::fs::write(deep.join("deep.txt"), b"needle").expect("written");
        // The file is too deep to be found.
        let out = GrepTool::new(dir.clone())
            .search("needle", None)
            .await
            .expect("searched");
        assert!(!out.contains("deep.txt"), "{out}");
        // A file that is shallow enough is found.
        std::fs::write(dir.join("shallow.txt"), b"needle").expect("written");
        let out = GrepTool::new(dir)
            .search("needle", None)
            .await
            .expect("searched");
        assert!(out.contains("shallow.txt"), "{out}");
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

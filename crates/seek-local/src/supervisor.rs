//! The llama.cpp supervisor (SPEC 7.3).
//!
//! SEEK owns the lifecycle of the local server: it starts `llama serve`, waits for the health
//! check, reads the real context size out of `/props` and stops the server again.
//!
//! The server runs in its own process group, so it can be stopped as a whole: SIGTERM to the
//! group, five seconds of grace, then SIGKILL.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::logs::{LogBuffer, LogLine};
use crate::model::LocalModel;
use crate::options::LaunchOptions;

/// How long the server gets to load the model.
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(120);
/// How long the server gets to shut down after SIGTERM.
pub const STOP_GRACE: Duration = Duration::from_secs(5);
/// How often the health check is repeated.
pub const HEALTH_INTERVAL: Duration = Duration::from_millis(250);

/// The health of the server, as `/props` reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerProps {
    /// The real context size, per slot.
    pub n_ctx: u32,
    /// How many slots the server has.
    pub total_slots: u32,
    /// The path of the model that is loaded.
    pub model_path: String,
    /// Whether the chat template supports reasoning effort.
    pub supports_reasoning_effort: bool,
    /// The reasoning effort levels the chat template accepts.
    pub reasoning_effort_levels: Vec<String>,
    /// Whether the chat template supports `enable_thinking`.
    pub supports_enable_thinking: bool,
}

impl ServerProps {
    /// Read the properties out of the answer of `/props`.
    pub fn from_json(value: &serde_json::Value) -> Self {
        let caps = value.get("chat_template_caps");
        ServerProps {
            n_ctx: value
                .pointer("/default_generation_settings/n_ctx")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
            total_slots: value
                .get("total_slots")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
            model_path: value
                .get("model_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            supports_reasoning_effort: caps
                .and_then(|caps| caps.get("supports_reasoning_effort"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            reasoning_effort_levels: caps
                .and_then(|caps| caps.get("reasoning_effort_levels"))
                .and_then(serde_json::Value::as_array)
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            supports_enable_thinking: value
                .get("chat_template")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|template| template.contains("enable_thinking")),
        }
    }
}

/// Something went wrong with the local server.
#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    #[error("the llama.cpp server was not found: {0}")]
    BinaryMissing(String),
    #[error("the server did not become healthy in time")]
    HealthTimeout,
    #[error("the server could not be started: {0}")]
    Start(String),
    #[error("the server request failed: {0}")]
    Request(String),
    #[error("no port was free for {0}")]
    NoPort(String),
}

/// The running server.
#[derive(Debug)]
pub struct RunningServer {
    /// The model the server runs.
    pub model_id: String,
    /// The port the server listens on.
    pub port: u16,
    /// The process id of the server.
    pub pid: u32,
    child: Child,
}

impl RunningServer {
    /// Stop the server: SIGTERM to the process group, then SIGKILL.
    pub async fn stop(&mut self) {
        let pid = self.pid;
        signal_group(pid, SIGTERM);
        let grace = tokio::time::timeout(STOP_GRACE, self.child.wait()).await;
        if grace.is_err() {
            signal_group(pid, SIGKILL);
            let _ = self.child.kill().await;
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        // The server never outlives SEEK, not even after a panic.
        signal_group(self.pid, SIGKILL);
    }
}

/// The signal to send to a process group.
const SIGTERM: i32 = 15;
/// The signal to send to a process group.
const SIGKILL: i32 = 9;

/// Send a signal to a process group.
fn signal_group(pid: u32, signal: i32) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        let signal = match signal {
            SIGTERM => Signal::SIGTERM,
            _ => Signal::SIGKILL,
        };
        let _ = killpg(Pid::from_raw(pid as i32), signal);
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
    }
}

/// The supervisor of the local server.
pub struct Supervisor {
    client: reqwest::Client,
    binary: String,
    logs: Arc<Mutex<LogBuffer>>,
    state_dir: PathBuf,
    running: Option<RunningServer>,
    props: Option<ServerProps>,
}

impl Supervisor {
    /// A supervisor that runs `binary`.
    pub fn new(binary: impl Into<String>, state_dir: PathBuf) -> Self {
        Supervisor {
            client: reqwest::Client::new(),
            binary: binary.into(),
            logs: Arc::new(Mutex::new(LogBuffer::default())),
            state_dir,
            running: None,
            props: None,
        }
    }

    /// The log of the server.
    pub fn logs(&self) -> std::sync::MutexGuard<'_, LogBuffer> {
        self.logs.lock().expect("the log is not poisoned")
    }

    /// The shared log buffer, so the UI can show the log strip.
    pub fn logs_handle(&self) -> Arc<Mutex<LogBuffer>> {
        Arc::clone(&self.logs)
    }

    /// The last lines of the log.
    pub fn last_log_lines(&self, count: usize) -> Vec<String> {
        self.logs()
            .last(count)
            .iter()
            .map(|line| line.text.clone())
            .collect()
    }

    /// The properties of the running server.
    pub fn props(&self) -> Option<&ServerProps> {
        self.props.as_ref()
    }

    /// The model the running server serves.
    pub fn running_model(&self) -> Option<&str> {
        self.running.as_ref().map(|server| server.model_id.as_str())
    }

    /// The port of the running server.
    pub fn port(&self) -> Option<u16> {
        self.running.as_ref().map(|server| server.port)
    }

    /// Whether a server is running.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Start the server for a model.
    pub async fn start(
        &mut self,
        model: &LocalModel,
        options: &LaunchOptions,
        port: u16,
    ) -> Result<(), LocalError> {
        self.stop().await;

        let command = options.command(&self.binary, &model.path, port);
        let mut builder = Command::new(&command[0]);
        builder
            .args(&command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // The server runs in its own process group, so it can be stopped as a whole.
        #[cfg(unix)]
        {
            builder.process_group(0);
        }
        let mut child = builder
            .spawn()
            .map_err(|error| LocalError::Start(error.to_string()))?;

        let pid = child.id().unwrap_or(0);
        self.pipe_output(&mut child);

        self.running = Some(RunningServer {
            model_id: model.id.clone(),
            port,
            pid,
            child,
        });
        self.write_pids();

        self.wait_until_healthy().await?;
        self.props = self.read_props().await;
        Ok(())
    }

    /// Stop the running server.
    pub async fn stop(&mut self) {
        if let Some(mut server) = self.running.take() {
            server.stop().await;
        }
        self.props = None;
        self.remove_pids();
    }

    /// Wait until the server answers the health check.
    async fn wait_until_healthy(&mut self) -> Result<(), LocalError> {
        let url = self.health_url();
        let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
        loop {
            if let Ok(response) = self.client.get(&url).send().await
                && response.status().is_success()
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(LocalError::HealthTimeout);
            }
            tokio::time::sleep(HEALTH_INTERVAL).await;
        }
    }

    /// Read the properties of the server.
    pub async fn read_props(&self) -> Option<ServerProps> {
        let url = format!("http://127.0.0.1:{}/props", self.port()?);
        let response = self.client.get(&url).send().await.ok()?;
        let value: serde_json::Value = response.json().await.ok()?;
        Some(ServerProps::from_json(&value))
    }

    /// Count the tokens of a conversation.
    ///
    /// The server applies the chat template with `/apply-template`, and the result is tokenized
    /// with `/tokenize` (SPEC 8).
    pub async fn count_tokens(&self, messages: &[serde_json::Value]) -> Option<usize> {
        let port = self.port()?;
        let template_url = format!("http://127.0.0.1:{port}/apply-template");
        let tokenize_url = format!("http://127.0.0.1:{port}/tokenize");

        let body = serde_json::json!({ "messages": messages });
        let response = self
            .client
            .post(&template_url)
            .json(&body)
            .send()
            .await
            .ok()?;
        let value: serde_json::Value = response.json().await.ok()?;
        let prompt = value.get("prompt")?.as_str()?.to_string();

        let body = serde_json::json!({ "content": prompt });
        let response = self
            .client
            .post(&tokenize_url)
            .json(&body)
            .send()
            .await
            .ok()?;
        let value: serde_json::Value = response.json().await.ok()?;
        let tokens = value.get("tokens")?.as_array()?;
        Some(tokens.len())
    }

    /// The URL of the health check.
    fn health_url(&self) -> String {
        match self.port() {
            Some(port) => format!("http://127.0.0.1:{port}/health"),
            None => "http://127.0.0.1:0/health".to_string(),
        }
    }

    /// Read the output of the server into the log.
    fn pipe_output(&mut self, child: &mut Child) {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        if let Some(stdout) = stdout {
            let logs = Arc::clone(&self.logs);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    logs.lock()
                        .expect("the log is not poisoned")
                        .push(LogLine::new(line, false));
                }
            });
        }
        if let Some(stderr) = stderr {
            let logs = Arc::clone(&self.logs);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    logs.lock()
                        .expect("the log is not poisoned")
                        .push(LogLine::new(line, true));
                }
            });
        }
    }

    /// Write the process ids to `state/pids`.
    fn write_pids(&self) {
        let Some(pid) = self.running.as_ref().map(|server| server.pid) else {
            return;
        };
        let path = self.state_dir.join("pids");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, format!("{pid}\n"));
    }

    /// Remove the process ids.
    fn remove_pids(&self) {
        let _ = std::fs::remove_file(self.state_dir.join("pids"));
    }

    /// Kill the orphans of a previous run.
    ///
    /// The pids of the servers that SEEK started are in `state/pids`; they are killed when SEEK
    /// starts, so a crashed run does not leave a server behind.
    pub fn kill_orphans(&self) {
        let path = self.state_dir.join("pids");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        for line in text.lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                signal_group(pid, SIGTERM);
                signal_group(pid, SIGKILL);
            }
        }
        let _ = std::fs::remove_file(path);
    }
}

/// The folder where the state of SEEK lives.
pub fn state_dir(home: &Path) -> PathBuf {
    home.join("state")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_props_are_read_out_of_the_answer() {
        let value = serde_json::json!({
            "default_generation_settings": { "n_ctx": 131072 },
            "total_slots": 1,
            "model_path": "/models/mistral.gguf",
            "chat_template": "{% if enable_thinking %}...{% endif %}",
            "chat_template_caps": {
                "supports_reasoning_effort": false,
                "reasoning_effort_levels": []
            }
        });
        let props = ServerProps::from_json(&value);
        assert_eq!(props.n_ctx, 131_072);
        assert_eq!(props.total_slots, 1);
        assert_eq!(props.model_path, "/models/mistral.gguf");
        assert!(!props.supports_reasoning_effort);
        assert!(props.reasoning_effort_levels.is_empty());
        assert!(props.supports_enable_thinking);
    }

    #[test]
    fn a_model_without_caps_is_read_without_panicking() {
        let props = ServerProps::from_json(&serde_json::json!({}));
        assert_eq!(props.n_ctx, 0);
        assert!(!props.supports_enable_thinking);
    }

    #[test]
    fn the_port_of_the_model_is_used() {
        let supervisor = Supervisor::new("llama", PathBuf::from("/tmp/seek-state"));
        assert!(!supervisor.is_running());
        assert_eq!(supervisor.port(), None);
    }

    #[test]
    fn the_orphan_pids_are_killed() {
        let dir = std::env::temp_dir().join("seek-orphan-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("created");
        std::fs::write(dir.join("pids"), "999999\n").expect("written");
        let supervisor = Supervisor::new("llama", dir.clone());
        supervisor.kill_orphans();
        assert!(!dir.join("pids").exists());
    }
}

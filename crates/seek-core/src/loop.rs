//! The agent loop (SPEC 9.1).
//!
//! Stream a completion, collect the tool calls, and for each of them: check the permission,
//! execute, append a `ToolResult`. Repeat until a response has no tool calls.
//!
//! Guards: a maximum number of iterations per turn (100 by default), a cancellation token on Esc,
//! a timeout per tool call, and errors that are returned to the model as `is_error` results
//! instead of crashing the turn.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc;

use crate::event::{AgentEvent, PermissionRequest, ToolOutput, TurnReason, Usage};
use crate::message::{Block, History, Message, Role};
use crate::permission::{PermissionDecision, PermissionMode, is_inside_project};
use crate::process::ProcessRegistry;
use crate::provider::{ChatRequest, Provider, ProviderEvent};
use crate::tool::{Risk, Tool, ToolError};
use crate::tools::{BashTool, EditTool, ReadTool, WriteTool};

/// The default number of iterations per turn.
pub const MAX_ITERATIONS: usize = 100;
/// The default timeout of a tool call.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

/// How the agent behaves.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// The maximum number of iterations per turn.
    pub max_iterations: usize,
    /// The timeout of a tool call.
    pub tool_timeout: Duration,
    /// The maximum number of output tokens of a response.
    pub max_tokens: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            max_iterations: MAX_ITERATIONS,
            tool_timeout: TOOL_TIMEOUT,
            max_tokens: None,
        }
    }
}

/// The tools the agent can call.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    pub read: Option<ReadTool>,
    pub write: Option<WriteTool>,
    pub edit: Option<EditTool>,
    pub bash: Option<BashTool>,
    /// The read-only helpers; they are off by default (SPEC 9.2).
    pub ls: Option<crate::tools::LsTool>,
    pub grep: Option<crate::tools::GrepTool>,
    pub find: Option<crate::tools::FindTool>,
    /// The network tools; they are off by default (SPEC 9.4).
    pub fetch: Option<crate::tools::FetchTool>,
    /// The web search; it is off by default (SPEC 9.4).
    pub web_search: Option<crate::websearch::WebSearchTool>,
}

/// The permission mode, shared by the turns.
///
/// The mode is read at every tool call, so the user can change it while the agent works:
/// the next call of the same turn follows the new mode.
#[derive(Debug, Clone)]
pub struct PermissionModeCell {
    mode: std::sync::Arc<std::sync::Mutex<PermissionMode>>,
}

impl PermissionModeCell {
    /// A cell with this mode.
    pub fn new(mode: PermissionMode) -> Self {
        PermissionModeCell {
            mode: std::sync::Arc::new(std::sync::Mutex::new(mode)),
        }
    }

    /// The mode of the moment.
    pub fn get(&self) -> PermissionMode {
        *self.mode.lock().expect("the mode is not poisoned")
    }

    /// Change the mode.
    pub fn set(&self, mode: PermissionMode) {
        *self.mode.lock().expect("the mode is not poisoned") = mode;
    }
}

/// The agent: a provider, the tools and the permission mode.
pub struct Agent {
    provider: Arc<dyn Provider>,
    tools: ToolRegistry,
    /// The permission mode, read at every tool call.
    mode: PermissionModeCell,
    root: std::path::PathBuf,
    config: AgentConfig,
    permissions: PermissionRequests,
    processes: Arc<ProcessRegistry>,
    /// What else goes into the prompt: the skills and the instructions of the project.
    context: String,
    /// The external tools of `~/seek/tools`.
    external_tools: Vec<crate::external::ExternalTool>,
    /// The tokens of the last turn.
    usage: std::sync::Arc<std::sync::Mutex<(u32, u32)>>,
    /// The token that stops the turn when the user presses Esc (SPEC 9.1).
    cancel: tokio_util::sync::CancellationToken,
}

impl Agent {
    /// Build an agent.
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        mode: PermissionMode,
        root: std::path::PathBuf,
        processes: Arc<ProcessRegistry>,
        permissions: PermissionRequests,
    ) -> Self {
        Agent {
            provider,
            tools,
            mode: PermissionModeCell::new(mode),
            root,
            config: AgentConfig::default(),
            permissions,
            processes,
            context: String::new(),
            external_tools: Vec::new(),
            usage: std::sync::Arc::new(std::sync::Mutex::new((0, 0))),
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Use a permission mode that is shared with the interface.
    ///
    /// The mode is read at every tool call, so the user can change it while the agent works.
    pub fn with_permission_mode(mut self, mode: PermissionModeCell) -> Self {
        self.mode = mode;
        self
    }

    /// The token that stops the turn.
    pub fn cancel_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancel.clone()
    }

    /// Use a token that was made outside.
    ///
    /// The binary keeps one token for the whole session, so an Esc that comes while the
    /// turn is still starting is not lost.
    pub fn with_cancel(mut self, token: tokio_util::sync::CancellationToken) -> Self {
        self.cancel = token;
        self
    }

    /// Where the tokens of the turn are counted.
    pub fn with_usage(mut self, usage: std::sync::Arc<std::sync::Mutex<(u32, u32)>>) -> Self {
        self.usage = usage;
        self
    }

    /// The external tools the agent may call.
    pub fn with_external_tools(mut self, tools: Vec<crate::external::ExternalTool>) -> Self {
        self.external_tools = tools;
        self
    }

    /// What else goes into the prompt.
    ///
    /// The catalog of the enabled skills and the instructions of the project (SPEC 9.5).
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }

    /// The system prompt of the request.
    fn system_prompt(&self) -> String {
        let mut prompt = crate::prompt::SYSTEM_PROMPT.to_string();
        if !self.context.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&self.context);
        }
        prompt
    }

    /// The tools the agent may call.
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// The process registry.
    pub fn processes(&self) -> &Arc<ProcessRegistry> {
        &self.processes
    }

    /// Run one turn.
    ///
    /// The turn ends when the model answers without tool calls, when the iteration limit is
    /// reached, or when the turn is cancelled.
    pub async fn run_turn(
        &self,
        history: &mut History,
        events: &mpsc::Sender<AgentEvent>,
    ) -> TurnReason {
        let _ = events.send(AgentEvent::TurnStarted).await;

        for _ in 0..self.config.max_iterations {
            if self.cancel.is_cancelled() {
                let _ = events
                    .send(AgentEvent::TurnFinished(TurnReason::Cancelled))
                    .await;
                return TurnReason::Cancelled;
            }
            let request = ChatRequest {
                messages: history.messages.clone(),
                tools: self.schemas(),
                max_tokens: self.config.max_tokens,
                system: Some(self.system_prompt()),
            };

            let mut text = String::new();
            let mut thinking = String::new();
            let mut calls: Vec<(String, String, serde_json::Value)> = Vec::new();

            let mut stream = self.provider.stream(request);
            while let Some(event) = tokio::select! {
                event = stream.next() => event,
                _ = self.cancel.cancelled() => {
                    let _ = events.send(AgentEvent::TurnFinished(TurnReason::Cancelled)).await;
                    return TurnReason::Cancelled;
                }
            } {
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        let _ = events.send(AgentEvent::Error(error.to_string())).await;
                        // The turn ends: the interface has to hear it, or it waits for a
                        // turn that is over for ever.
                        let _ = events
                            .send(AgentEvent::TurnFinished(TurnReason::Failed))
                            .await;
                        return TurnReason::Failed;
                    }
                };
                match event {
                    ProviderEvent::TextDelta(delta) => {
                        text.push_str(&delta);
                        let _ = events.send(AgentEvent::TextDelta(delta)).await;
                    }
                    ProviderEvent::ThinkingDelta(delta) => {
                        thinking.push_str(&delta);
                        let _ = events.send(AgentEvent::ThinkingDelta(delta)).await;
                    }
                    ProviderEvent::ToolCall { id, name, input } => {
                        let _ = events
                            .send(AgentEvent::ToolCallStarted {
                                id: id.clone(),
                                name: name.clone(),
                            })
                            .await;
                        let _ = events
                            .send(AgentEvent::ToolCallReady {
                                id: id.clone(),
                                name: name.clone(),
                                input: input.clone(),
                            })
                            .await;
                        calls.push((id, name, input));
                    }
                    ProviderEvent::Usage { input, output } => {
                        *self.usage.lock().expect("the usage is not poisoned") = (input, output);
                        let _ = events
                            .send(AgentEvent::Usage(Usage { input, output }))
                            .await;
                    }
                    ProviderEvent::RateLimit(limit) => {
                        let _ = events.send(AgentEvent::RateLimit(limit)).await;
                    }
                    ProviderEvent::Done => {}
                }
            }

            // The assistant message is appended to the history, which stays append-only.
            let mut blocks = Vec::new();
            if !thinking.is_empty() {
                blocks.push(Block::thinking(self.provider.name(), thinking));
            }
            if !text.is_empty() {
                blocks.push(Block::text(text.clone()));
            }
            for (id, name, input) in &calls {
                blocks.push(Block::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            if !blocks.is_empty() {
                history.push(Message::new(Role::Assistant, blocks));
            }

            if calls.is_empty() {
                let _ = events
                    .send(AgentEvent::TurnFinished(TurnReason::Stop))
                    .await;
                return TurnReason::Stop;
            }

            for (id, name, input) in calls {
                let outcome = self.run_tool(&name, input).await;

                let (content, is_error) = match outcome {
                    Ok(content) => (content, false),
                    Err(content) => (content, true),
                };

                history.push(Message::new(
                    Role::User,
                    vec![Block::ToolResult {
                        id: id.clone(),
                        content: content.clone(),
                        is_error,
                    }],
                ));

                let _ = events
                    .send(AgentEvent::ToolOutput(ToolOutput {
                        id,
                        name,
                        content,
                        is_error,
                        duration: Duration::ZERO,
                    }))
                    .await;
            }
        }

        let _ = events
            .send(AgentEvent::TurnFinished(TurnReason::MaxIterations))
            .await;
        TurnReason::MaxIterations
    }

    /// Check the permission, then run the tool.
    async fn run_tool(&self, name: &str, input: serde_json::Value) -> Result<String, String> {
        let risk = self.risk_of(name);
        let path = input
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(|path| self.root.join(path));
        let inside = match &path {
            Some(path) => is_inside_project(&self.root, path),
            None => true,
        };

        match self.mode.get().decide_path(risk, inside) {
            crate::permission::Decision::Allow => {}
            crate::permission::Decision::Deny => {
                return Err("denied by user".to_string());
            }
            crate::permission::Decision::Ask => {
                let summary = summarize(name, &input);
                let decision = self
                    .permissions
                    .request_until(name.to_string(), summary, Some(&self.cancel))
                    .await;
                if let PermissionDecision::Allow = decision {
                    // Allowed.
                } else {
                    return Err(decision.denial_text());
                }
            }
        }

        // A tool that runs for ever would keep the turn from ending: Esc has to open the
        // way out. The command itself stays in the registry of the processes, so the user
        // can kill it there.
        tokio::select! {
            outcome = self.execute(name, input) => outcome,
            _ = self.cancel.cancelled() => Err("the turn was cancelled".to_string()),
        }
    }

    /// Run the tool itself.
    async fn execute(&self, name: &str, input: serde_json::Value) -> Result<String, String> {
        let call = async {
            match name {
                "read" => {
                    let tool = self
                        .tools
                        .read
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("read is off".into()))?;
                    tool.call(input).await
                }
                "write" => {
                    let tool = self
                        .tools
                        .write
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("write is off".into()))?;
                    tool.call(input).await
                }
                "edit" => {
                    let tool = self
                        .tools
                        .edit
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("edit is off".into()))?;
                    tool.call(input).await
                }
                "bash" => {
                    let tool = self
                        .tools
                        .bash
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("bash is off".into()))?;
                    tool.call(input).await
                }
                "ls" => {
                    let tool = self
                        .tools
                        .ls
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("ls is off".into()))?;
                    tool.call(input).await
                }
                "grep" => {
                    let tool = self
                        .tools
                        .grep
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("grep is off".into()))?;
                    tool.call(input).await
                }
                "find" => {
                    let tool = self
                        .tools
                        .find
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("find is off".into()))?;
                    tool.call(input).await
                }
                "fetch" => {
                    let tool = self
                        .tools
                        .fetch
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("fetch is off".into()))?;
                    tool.call(input).await
                }
                "web_search" => {
                    let tool = self
                        .tools
                        .web_search
                        .as_ref()
                        .ok_or_else(|| ToolError::NotFound("web_search is off".into()))?;
                    tool.call(input).await
                }
                other => {
                    // An external tool runs its own executable.
                    let Some(tool) = self
                        .external_tools
                        .iter()
                        .find(|tool| tool.config.name == other)
                    else {
                        return Err(ToolError::NotFound(other.to_string()));
                    };
                    match tool.run(&input).await {
                        Ok(content) => Ok(ToolOutput {
                            id: String::new(),
                            name: tool.config.name.clone(),
                            content,
                            is_error: false,
                            duration: Duration::ZERO,
                        }),
                        Err(error) => Err(error),
                    }
                }
            }
        };

        match tokio::time::timeout(self.config.tool_timeout, call).await {
            Ok(Ok(output)) => Ok(output.content),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err(format!(
                "the tool did not finish within {:?}",
                self.config.tool_timeout
            )),
        }
    }

    /// The risk of a tool.
    fn risk_of(&self, name: &str) -> Risk {
        match name {
            "read" => Risk::Read,
            "write" | "edit" => Risk::Write,
            "bash" => Risk::Exec,
            _ => Risk::Network,
        }
    }

    /// The schemas of the enabled tools.
    ///
    /// Only the enabled tools are sent (SPEC 9.2).
    fn schemas(&self) -> Vec<crate::tool::ToolSchema> {
        let mut schemas = Vec::new();
        if self.tools.read.is_some() {
            schemas.push(crate::tools::read_schema());
        }
        if self.tools.write.is_some() {
            schemas.push(crate::tools::write_schema());
        }
        if self.tools.edit.is_some() {
            schemas.push(crate::tools::edit_schema());
        }
        if self.tools.bash.is_some() {
            schemas.push(crate::tools::bash_schema());
        }
        if self.tools.ls.is_some() {
            schemas.push(crate::tools::ls_schema());
        }
        if self.tools.grep.is_some() {
            schemas.push(crate::tools::grep_schema());
        }
        if self.tools.find.is_some() {
            schemas.push(crate::tools::find_schema());
        }
        if self.tools.fetch.is_some() {
            schemas.push(crate::tools::fetch_schema());
        }
        for tool in &self.external_tools {
            schemas.push(tool.tool_schema());
        }
        schemas
    }
}

/// A one-line summary of a tool call.
fn summarize(name: &str, input: &serde_json::Value) -> String {
    for key in ["command", "path", "pattern"] {
        if let Some(value) = input.get(key).and_then(serde_json::Value::as_str) {
            return format!("{name} {value}");
        }
    }
    name.to_string()
}

/// The permission requests of the agent (SPEC 9.4).
///
/// The agent sends a request and waits for the answer; the UI shows the prompt bar with the
/// three choices and the answer comes back through [`PermissionRequests::answer`].
#[derive(Clone)]
pub struct PermissionRequests {
    requests: mpsc::Sender<PermissionRequest>,
    pending: Arc<
        std::sync::Mutex<
            std::collections::HashMap<u64, tokio::sync::oneshot::Sender<PermissionDecision>>,
        >,
    >,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl std::fmt::Debug for PermissionRequests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionRequests").finish_non_exhaustive()
    }
}

impl Default for PermissionRequests {
    fn default() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        PermissionRequests {
            requests: tx,
            pending: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }
}

impl PermissionRequests {
    /// A channel: the sender for the agent, the receiver for the UI.
    pub fn new() -> (Self, mpsc::Receiver<PermissionRequest>) {
        let (tx, rx) = mpsc::channel(16);
        (
            PermissionRequests {
                requests: tx,
                pending: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
                next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            },
            rx,
        )
    }

    /// Ask the user and wait for the answer.
    pub async fn request(&self, tool: String, summary: String) -> PermissionDecision {
        self.request_until(tool, summary, None).await
    }

    /// Ask the user and wait for the answer, unless the turn is cancelled first.
    ///
    /// A turn that waits for an answer that never comes would hang for ever: Esc has to
    /// open the way out. The request is taken out of the list when the turn is cancelled,
    /// so a late answer does not wait for a caller that is gone.
    pub async fn request_until(
        &self,
        tool: String,
        summary: String,
        cancel: Option<&tokio_util::sync::CancellationToken>,
    ) -> PermissionDecision {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .expect("the map is not poisoned")
            .insert(id, tx);
        if self
            .requests
            .send(PermissionRequest { id, tool, summary })
            .await
            .is_err()
        {
            return PermissionDecision::Deny;
        }
        match cancel {
            None => rx.await.unwrap_or(PermissionDecision::Deny),
            Some(token) => {
                tokio::select! {
                    decision = rx => decision.unwrap_or(PermissionDecision::Deny),
                    _ = token.cancelled() => {
                        // The request is taken out: the answer of the user finds nobody.
                        self.pending.lock().expect("the map is not poisoned").remove(&id);
                        PermissionDecision::Deny
                    }
                }
            }
        }
    }

    /// Answer a request.
    pub fn answer(&self, id: u64, decision: PermissionDecision) {
        if let Some(tx) = self
            .pending
            .lock()
            .expect("the map is not poisoned")
            .remove(&id)
        {
            let _ = tx.send(decision);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Protocol;
    use std::sync::Mutex;

    /// A provider that replays a script of responses.
    struct ScriptedProvider {
        responses: Mutex<Vec<Vec<ProviderEvent>>>,
        requests: Mutex<Vec<ChatRequest>>,
    }

    impl ScriptedProvider {
        fn new(responses: Vec<Vec<ProviderEvent>>) -> Self {
            ScriptedProvider {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl Provider for ScriptedProvider {
        fn name(&self) -> &str {
            "scripted"
        }

        fn model(&self) -> &str {
            "scripted/model"
        }

        fn protocol(&self) -> Protocol {
            Protocol::OpenAiChat
        }

        fn stream<'a>(&'a self, request: ChatRequest) -> crate::provider::BoxProviderStream<'a> {
            self.requests.lock().unwrap().push(request);
            let next = self.responses.lock().unwrap().remove(0);
            futures::stream::iter(next.into_iter().map(Ok)).boxed()
        }
    }

    fn agent(
        provider: Arc<dyn Provider>,
        mode: PermissionMode,
    ) -> (Agent, mpsc::Receiver<PermissionRequest>, PermissionRequests) {
        let root = std::env::temp_dir().join("seek-agent-test");
        let _ = std::fs::create_dir_all(&root);
        let processes = Arc::new(ProcessRegistry::new());
        let tools = ToolRegistry {
            read: Some(ReadTool::new(root.clone())),
            write: Some(WriteTool::new(root.clone())),
            edit: Some(EditTool::new(root.clone())),
            bash: Some(BashTool::new(root.clone(), processes.clone())),
            ls: None,
            grep: None,
            find: None,
            fetch: None,
            web_search: None,
        };
        let (permissions, requests) = PermissionRequests::new();
        (
            Agent::new(provider, tools, mode, root, processes, permissions.clone()),
            requests,
            permissions,
        )
    }

    #[tokio::test]
    async fn a_turn_without_tool_calls_ends() {
        let provider = Arc::new(ScriptedProvider::new(vec![vec![
            ProviderEvent::TextDelta("hello".into()),
            ProviderEvent::Done,
        ]]));
        let (agent, _requests, _permissions) = agent(provider, PermissionMode::Yolo);
        let (tx, mut rx) = mpsc::channel(32);

        let mut history = History::new();
        history.push(Message::user("hi"));
        let reason = agent.run_turn(&mut history, &tx).await;

        assert_eq!(reason, TurnReason::Stop);
        assert_eq!(history.messages.len(), 2);
        assert_eq!(history.messages[1].plain_text(), "hello");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::TurnStarted))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::TurnFinished(_)))
        );
    }

    #[tokio::test]
    async fn a_tool_call_is_executed_and_the_result_goes_back() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            vec![ProviderEvent::ToolCall {
                id: "t1".into(),
                name: "read".into(),
                input: serde_json::json!({ "path": "a.txt" }),
            }],
            vec![ProviderEvent::TextDelta("done".into())],
        ]));
        let root = std::env::temp_dir().join("seek-agent-test");
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(root.join("a.txt"), "content").unwrap();

        let (agent, _requests, _permissions) = agent(provider, PermissionMode::Yolo);
        let (tx, _rx) = mpsc::channel(32);

        let mut history = History::new();
        let reason = agent.run_turn(&mut history, &tx).await;

        assert_eq!(reason, TurnReason::Stop);
        // The assistant message with the tool call, the tool result and the answer.
        let result = history
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                Block::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("there is a tool result");
        assert!(result.contains("content"));
    }

    #[tokio::test]
    async fn a_denied_tool_is_not_run() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            vec![ProviderEvent::ToolCall {
                id: "t1".into(),
                name: "write".into(),
                input: serde_json::json!({ "path": "x.txt", "content": "no" }),
            }],
            // The result goes back to the model, which answers again.
            vec![ProviderEvent::TextDelta("understood".into())],
        ]));
        let (agent, _requests, _permissions) = agent(provider, PermissionMode::Read);
        let (tx, _rx) = mpsc::channel(32);

        let mut history = History::new();
        agent.run_turn(&mut history, &tx).await;

        // The assistant message with the tool call, then the tool result.
        let result = history
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                Block::ToolResult {
                    content, is_error, ..
                } => Some((content.clone(), *is_error)),
                _ => None,
            })
            .expect("there is a tool result");
        assert!(result.0.contains("denied"));
        assert!(result.1);
    }

    #[tokio::test]
    async fn a_permission_request_waits_for_the_answer() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            vec![ProviderEvent::ToolCall {
                id: "t1".into(),
                name: "write".into(),
                input: serde_json::json!({ "path": "x.txt", "content": "yes" }),
            }],
            // The result goes back to the model, which answers again.
            vec![ProviderEvent::TextDelta("written".into())],
        ]));
        let (agent, mut requests, permissions) = agent(provider, PermissionMode::Ask);
        let (tx, _rx) = mpsc::channel(32);

        let handle = tokio::spawn({
            let agent = Arc::new(agent);
            async move {
                let mut history = History::new();
                agent.run_turn(&mut history, &tx).await
            }
        });

        let request = requests.recv().await.expect("a request is sent");
        assert_eq!(request.tool, "write");
        assert!(request.summary.contains("x.txt"));

        permissions.answer(request.id, PermissionDecision::Allow);
        let reason = handle.await.expect("the turn finishes");
        assert_eq!(reason, TurnReason::Stop);
    }
}

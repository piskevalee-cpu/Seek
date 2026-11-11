//! The wiring of SEEK: the runtime, the agent task and the checks (SPEC 4.1).
//!
//! The binary in `main.rs` is only the command line; everything that can be tested lives here.

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};

use seek_core::r#loop::{Agent, PermissionRequests, ToolRegistry};
use seek_core::process::ProcessRegistry;
use seek_core::tools::{BashTool, EditTool, ReadTool, WriteTool};
use seek_core::{AgentEvent, History, ProvidersConfig};
use seek_llm::FakeProvider;
use seek_llm::anthropic::AnthropicProvider;
use seek_llm::openai::OpenAiProvider;
use seek_llm::provider::{ChatRequest, Provider, ProviderEvent};
use seek_tui::app::Command;
use tokio::sync::mpsc;

pub mod doctor;

pub use seek_local::model::LocalModel;
pub use seek_local::options::LaunchOptions;
pub use seek_local::supervisor::Supervisor;

/// The state the binary keeps between turns.
pub struct Runtime {
    /// The supervisor of the local server.
    pub supervisor: tokio::sync::Mutex<Supervisor>,
    /// The local models of `~/seek/models`.
    pub local_models: Vec<LocalModel>,
    /// The model the user picked.
    current_model: Mutex<Option<String>>,
    /// The installed skills.
    pub skills: Vec<seek_core::Skill>,
    /// Which tools and skills are enabled.
    pub toggles: seek_core::Toggles,
    /// The instructions of the project: the AGENTS.md of the working directory.
    pub project_context: String,
    /// The external tools of `~/seek/tools`.
    pub external_tools: Vec<seek_core::ExternalTool>,
    /// The widgets of `~/seek/widgets`.
    pub widgets: Vec<seek_core::Widget>,
    /// The SEEK home.
    pub home: std::path::PathBuf,
    /// The session that is open.
    session: tokio::sync::Mutex<Option<seek_core::Session>>,
    /// The token that stops the turn that is running.
    cancel: tokio::sync::Mutex<Option<tokio_util::sync::CancellationToken>>,
    /// Where the state of SEEK lives.
    pub state_dir: std::path::PathBuf,
}

impl Runtime {
    /// The runtime for a SEEK home.
    pub fn new(home: &Path, binary: &str, subcommand: Option<String>) -> Self {
        let models_dir = home.join(seek_local::MODELS_DIR);
        let local_models = seek_local::model::scan(&models_dir);
        let state_dir = seek_local::supervisor::state_dir(home);
        let supervisor = Supervisor::new(binary.to_string(), subcommand, state_dir.clone());
        // A server of a previous run does not survive SEEK.
        supervisor.kill_orphans();

        // The skills of ~/seek/skills, and the built-in ones that are not installed yet.
        let mut skills = seek_core::skill::scan(&home.join("skills"));
        for skill in seek_core::skill::scan(&builtin_skills()) {
            if !skills.iter().any(|installed| installed.name == skill.name) {
                skills.push(skill);
            }
        }
        let toggles = seek_core::Toggles::read(&state_dir.join("toggles.toml")).unwrap_or_default();

        // The instructions of the project, if there are any (SPEC 9.5).
        let project_context = std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join("AGENTS.md"))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();

        // The external tools of ~/seek/tools and the widgets of ~/seek/widgets.
        let external_tools = seek_core::external::scan(&home.join("tools"));
        let mut widgets = seek_core::widget::scan(&home.join("widgets"));
        for widget in seek_core::widget::scan(&builtin_widgets()) {
            if !widgets
                .iter()
                .any(|installed| installed.config.name == widget.config.name)
            {
                widgets.push(widget);
            }
        }

        Runtime {
            home: home.to_path_buf(),
            supervisor: tokio::sync::Mutex::new(supervisor),
            local_models,
            current_model: Mutex::new(None),
            skills,
            toggles,
            project_context,
            external_tools,
            widgets,
            session: tokio::sync::Mutex::new(None),
            cancel: tokio::sync::Mutex::new(None),
            state_dir,
        }
    }

    /// The folder the sessions live in.
    fn sessions_dir(&self) -> std::path::PathBuf {
        self.state_dir.join("sessions")
    }

    /// Start a new session.
    pub async fn new_session(&self) -> std::io::Result<()> {
        let dir = self.sessions_dir();
        std::fs::create_dir_all(&dir)?;
        let session = seek_core::Session::create(
            &dir,
            &std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            &seek_core::Session::now(),
            &seek_core::Session::new_id(),
        )?;
        *self.session.lock().await = Some(session);
        Ok(())
    }

    /// Open a session and return its messages.
    pub async fn resume_session(&self, id: &str) -> std::io::Result<seek_core::History> {
        let path = self.sessions_dir().join(format!("{id}.jsonl"));
        let session = seek_core::Session::open(&path)?;
        let history = session.replay()?;
        *self.session.lock().await = Some(session);
        Ok(history)
    }

    /// Append a message to the open session.
    ///
    /// The session is created when the first message is written, so that an empty start does not
    /// leave a file behind.
    pub async fn append(&self, message: &seek_core::Message) {
        if self.session.lock().await.is_none() {
            let _ = self.new_session().await;
        }
        let mut session = self.session.lock().await;
        let Some(session) = session.as_mut() else {
            return;
        };
        let model = self.current_model().map(|id| {
            let local = id.starts_with("local/");
            seek_core::ModelTag {
                provider: if local {
                    "local".into()
                } else {
                    "cloud".into()
                },
                model: id,
                local,
            }
        });
        let _ = session.append_message(message, model.as_ref());
    }

    /// Record a model switch in the session.
    pub async fn append_switch(&self, model: &str) {
        let mut session = self.session.lock().await;
        let Some(session) = session.as_mut() else {
            return;
        };
        let _ = session.append_model_switch(&seek_core::ModelTag {
            provider: if model.starts_with("local/") {
                "local".into()
            } else {
                "cloud".into()
            },
            model: model.to_string(),
            local: model.starts_with("local/"),
        });
    }

    /// The token that stops the turn that is running.
    pub async fn set_cancel(&self, token: tokio_util::sync::CancellationToken) {
        *self.cancel.lock().await = Some(token);
    }

    /// Stop the turn that is running.
    pub async fn cancel(&self) {
        if let Some(token) = self.cancel.lock().await.as_ref() {
            token.cancel();
        }
    }

    /// The context that goes into the prompt: the catalog of the skills and the instructions of
    /// the project.
    pub fn context(&self) -> String {
        let mut context =
            seek_core::skill::catalog(&self.skills, &|name| self.toggles.skill_enabled(name));
        if !self.project_context.is_empty() {
            if !context.is_empty() {
                context.push_str("\n\n");
            }
            context.push_str("Instructions of the project (AGENTS.md):\n");
            context.push_str(&self.project_context);
        }
        context
    }

    /// Write the toggles to the state.
    pub fn save_toggles(&self, toggles: &seek_core::Toggles) {
        let _ = toggles.write(&self.state_dir.join("toggles.toml"));
    }

    /// Add to the lifetime usage and write it to the state.
    ///
    /// The `usage` widget of the home screen reads it.
    pub fn add_usage(&self, input: u32, output: u32) {
        let path = self.state_dir.join("usage.toml");
        let mut usage = LifetimeUsage::read(&path);
        usage.input = usage.input.saturating_add(input);
        usage.output = usage.output.saturating_add(output);
        let _ = usage.write(&path);
    }

    /// The model the user picked.
    pub fn current_model(&self) -> Option<String> {
        self.current_model
            .lock()
            .expect("the model is not poisoned")
            .clone()
    }

    /// Remember the model the user picked.
    pub fn set_current_model(&self, id: Option<String>) {
        *self
            .current_model
            .lock()
            .expect("the model is not poisoned") = id;
    }

    /// The local model with that id.
    pub fn local_model(&self, id: &str) -> Option<LocalModel> {
        self.local_models
            .iter()
            .find(|model| model.id == id)
            .cloned()
    }

    /// Make sure the server of the local model runs, and return its port.
    ///
    /// The server of another model is stopped first (SPEC 7.3).
    pub async fn ensure_local_server(&self, id: &str) -> Result<u16, String> {
        let Some(model) = self.local_model(id) else {
            return Err(format!("the local model {id} is gone"));
        };

        let mut supervisor = self.supervisor.lock().await;

        if supervisor.running_model() != Some(id) {
            supervisor.stop().await;
        } else if let Some(port) = supervisor.port() {
            return Ok(port);
        }

        // The port of the model is remembered in its sidecar.
        let port = match model.sidecar.port {
            Some(port) => port,
            None => {
                let port = find_free_port(id).ok_or_else(|| format!("no free port for {id}"))?;
                let mut sidecar = model.sidecar.clone();
                sidecar.port = Some(port);
                let _ = sidecar.write(&sidecar_path(&model.path));
                port
            }
        };

        let options = LaunchOptions::parse(model.sidecar.options.as_deref().unwrap_or_default());
        supervisor
            .start(&model, &options, port)
            .await
            .map_err(|error| error.to_string())?;
        Ok(port)
    }
}

/// The sidecar of a model.
pub use seek_local::model::sidecar_path;

/// The port of a model, when it is free.
pub fn find_free_port(model_id: &str) -> Option<u16> {
    seek_local::port::find_free_port(model_id)
}

/// Run the agent: every submitted message becomes one turn.
pub async fn agent_loop(
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<AgentEvent>,
    widgets: mpsc::Sender<seek_tui::app::UiMessage>,
    providers: ProvidersConfig,
    permissions: PermissionRequests,
    runtime: Arc<Runtime>,
) {
    use futures::StreamExt;

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let processes = Arc::new(ProcessRegistry::new());

    while let Some(command) = commands.recv().await {
        match command {
            Command::Quit => break,
            Command::SetToggles(toggles) => {
                runtime.save_toggles(&toggles);
            }
            Command::PickerClosed { toggles, model } => {
                runtime.save_toggles(&toggles);
                if let Some(model) = model {
                    runtime.set_current_model(Some(model));
                }
            }
            Command::NewSession => {
                // A new session: the file is new and the transcript starts empty (SPEC 11).
                if runtime.new_session().await.is_err() {
                    let _ = events
                        .send(AgentEvent::Error("the session could not be created".into()))
                        .await;
                }
                let sessions = seek_core::Session::list(&runtime.state_dir.join("sessions"));
                let _ = widgets
                    .send(seek_tui::app::UiMessage::Sessions(sessions))
                    .await;
            }
            Command::ResumeSession(id) => {
                // The messages of the session go back to the UI (SPEC 11).
                if let Ok(history) = runtime.resume_session(&id).await {
                    let _ = widgets
                        .send(seek_tui::app::UiMessage::Session(history.messages))
                        .await;
                }
            }
            Command::AddProvider(config) => {
                // The provider is added to the configuration and written to the file.
                let mut providers = providers.clone();
                let id = config
                    .name
                    .clone()
                    .unwrap_or_default()
                    .to_lowercase()
                    .replace(' ', "-");
                providers.provider.insert(id, config);
                let path = runtime.home.join(seek_core::config::PROVIDERS_FILE);
                if let Err(error) = providers.write(&path) {
                    let _ = events
                        .send(AgentEvent::Error(format!(
                            "the provider could not be saved: {error}"
                        )))
                        .await;
                } else {
                    let sessions = seek_core::Session::list(&runtime.state_dir.join("sessions"));
                    let _ = widgets
                        .send(seek_tui::app::UiMessage::Sessions(sessions))
                        .await;
                }
            }
            Command::Cancel => {
                // Esc stops the turn that is running (SPEC 9.1).
                runtime.cancel().await;
            }
            Command::RunWidgets => {
                // The widgets run when the home screen is shown (SPEC 10.5).
                for widget in &runtime.widgets {
                    let output = widget.run().await;
                    if widgets
                        .send(seek_tui::app::UiMessage::Widget(output))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Command::Permission { id, decision } => {
                permissions.answer(id, decision);
            }
            Command::Compact { messages } => {
                // The model summarizes the older turns into one message (SPEC 8).
                match summarize(&providers, runtime.current_model().as_deref(), messages).await {
                    Ok(summary) => {
                        let _ = events.send(AgentEvent::Compacted(summary)).await;
                    }
                    Err(error) => {
                        let _ = events.send(AgentEvent::Error(error)).await;
                    }
                }
            }
            Command::SwitchModel { id } => {
                // A model switch happens between turns (SPEC 7.3).
                runtime.set_current_model(Some(id.clone()));
                runtime.append_switch(&id).await;
            }
            Command::Submit { text, mode } => {
                // Only the enabled tools are given to the agent (SPEC 9.2, 9.6).
                let toggles = &runtime.toggles;
                let tools = ToolRegistry {
                    read: toggles
                        .tool_enabled("read")
                        .then(|| ReadTool::new(root.clone())),
                    write: toggles
                        .tool_enabled("write")
                        .then(|| WriteTool::new(root.clone())),
                    edit: toggles
                        .tool_enabled("edit")
                        .then(|| EditTool::new(root.clone())),
                    bash: toggles
                        .tool_enabled("bash")
                        .then(|| BashTool::new(root.clone(), processes.clone())),
                    ls: toggles
                        .tool_enabled("ls")
                        .then(|| seek_core::tools::LsTool::new(root.clone())),
                    grep: toggles
                        .tool_enabled("grep")
                        .then(|| seek_core::tools::GrepTool::new(root.clone())),
                    find: toggles
                        .tool_enabled("find")
                        .then(|| seek_core::tools::FindTool::new(root.clone())),
                    fetch: toggles
                        .tool_enabled("fetch")
                        .then(seek_core::tools::FetchTool::new),
                };
                let external_tools: Vec<_> = runtime
                    .external_tools
                    .iter()
                    .filter(|tool| toggles.tool_enabled(&tool.config.name))
                    .cloned()
                    .collect();

                let mut history = History::new();
                let user_message = seek_core::Message::user(text);
                history.push(user_message.clone());
                // The conversation is written to the session (SPEC 11).
                runtime.append(&user_message).await;
                // The usage of the turn is counted for the lifetime usage.
                let usage = Arc::new(Mutex::new((0, 0)));

                // A local model that cannot be started is an error: the scripted provider
                // would answer as if nothing had happened.
                let mut local_failed = false;
                let provider = match runtime.current_model() {
                    Some(id) if id.starts_with("local/") => {
                        match runtime.ensure_local_server(&id).await {
                            Ok(port) => {
                                // The real context size of the server goes to the metrics
                                // (SPEC 8).
                                if let Some(props) =
                                    runtime.supervisor.lock().await.read_props().await
                                {
                                    let context = props.n_ctx;
                                    let _ = widgets
                                        .send(seek_tui::app::UiMessage::Context(context))
                                        .await;
                                }
                                Some(Box::new(OpenAiProvider::new(
                                    format!("http://127.0.0.1:{port}/v1"),
                                    None,
                                    local_model_name(&id),
                                    8_192,
                                )) as Box<dyn Provider>)
                            }
                            Err(error) => {
                                let _ = events
                                    .send(AgentEvent::Error(format!(
                                        "the local model could not be started: {error}"
                                    )))
                                    .await;
                                local_failed = true;
                                None
                            }
                        }
                    }
                    _ => build_provider(&providers),
                };

                match provider {
                    Some(provider) => {
                        let agent = Agent::new(
                            Arc::from(provider),
                            tools,
                            mode,
                            root.clone(),
                            processes.clone(),
                            permissions.clone(),
                        )
                        .with_context(runtime.context())
                        .with_external_tools(external_tools)
                        .with_usage(Arc::clone(&usage));
                        // The token lets Esc stop the turn (SPEC 9.1).
                        let cancel = agent.cancel_token();
                        runtime.set_cancel(cancel.clone()).await;
                        agent.run_turn(&mut history, &events).await;
                        runtime
                            .set_cancel(tokio_util::sync::CancellationToken::new())
                            .await;
                        // The answer goes into the session as well.
                        if let Some(answer) = history.messages.last() {
                            runtime.append(answer).await;
                        }
                        let (input, output) = *usage.lock().expect("the usage is not poisoned");
                        runtime.add_usage(input, output);
                    }
                    None if local_failed => {
                        // The error is already in the transcript.
                    }
                    None => {
                        // No provider is configured: the scripted provider keeps the UI alive.
                        let provider = FakeProvider::demo();
                        let request = ChatRequest {
                            messages: history.messages.clone(),
                            ..ChatRequest::default()
                        };
                        let mut stream = provider.stream(request);
                        let _ = events.send(AgentEvent::TurnStarted).await;
                        while let Some(event) = stream.next().await {
                            let Some(event) = event.ok() else { break };
                            let agent_event = match event {
                                ProviderEvent::ThinkingDelta(text) => {
                                    AgentEvent::ThinkingDelta(text)
                                }
                                ProviderEvent::TextDelta(text) => AgentEvent::TextDelta(text),
                                ProviderEvent::Usage { input, output } => {
                                    AgentEvent::Usage(seek_core::Usage { input, output })
                                }
                                ProviderEvent::RateLimit(limit) => AgentEvent::RateLimit(limit),
                                ProviderEvent::Done => {
                                    AgentEvent::TurnFinished(seek_core::TurnReason::Stop)
                                }
                                ProviderEvent::ToolCall { .. } => continue,
                            };
                            if events.send(agent_event).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Ask the model to summarize a conversation.
async fn summarize(
    providers: &ProvidersConfig,
    current_model: Option<&str>,
    messages: Vec<seek_core::Message>,
) -> Result<String, String> {
    use futures::StreamExt;

    let provider = match current_model {
        Some(id) if id.starts_with("local/") => None,
        _ => build_provider(providers),
    };
    let Some(provider) = provider else {
        return Err("a provider is needed to compact the conversation".to_string());
    };

    let mut request = ChatRequest {
        messages,
        ..ChatRequest::default()
    };
    request.messages.push(seek_core::Message::user(
        "Summarize the conversation above in a few sentences. Keep the decisions, the file paths \
         and the open questions; drop the small talk.",
    ));

    let mut stream = provider.stream(request);
    let mut summary = String::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(ProviderEvent::TextDelta(text)) => summary.push_str(&text),
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    if summary.trim().is_empty() {
        return Err("the model did not summarize anything".to_string());
    }
    Ok(summary)
}

/// Build a provider from the configuration.
///
/// The first provider that has a key in the environment wins. Without one, SEEK runs on the
/// scripted provider.
pub fn build_provider(providers: &ProvidersConfig) -> Option<Box<dyn Provider>> {
    for (id, provider) in &providers.provider {
        let Some(key) = provider.api_key() else {
            continue;
        };
        let Some(base_url) = &provider.base_url else {
            continue;
        };
        let model = provider
            .models
            .iter()
            .find(|(_, model)| !model.disabled.unwrap_or(false))
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| id.clone());

        return Some(match provider.provider_kind() {
            Some(seek_core::ProviderKind::AnthropicMessages) => {
                Box::new(AnthropicProvider::new(base_url.clone(), key, model, 8_192))
            }
            _ => Box::new(OpenAiProvider::new(
                base_url.clone(),
                Some(key),
                model,
                8_192,
            )),
        });
    }
    None
}

/// The folder with the built-in skills.
///
/// The skills are shipped with the binary and copied into `~/seek/skills` by the install script.
pub fn builtin_skills() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("SEEK_BUILTIN") {
        return std::path::PathBuf::from(path);
    }
    // The binary lives in target/{debug,release}; the skills are two levels up.
    if let Ok(exe) = std::env::current_exe()
        && let Some(folder) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
    {
        let candidate = folder.join("builtin");
        if candidate.is_dir() {
            return candidate;
        }
    }
    std::path::PathBuf::from("builtin")
}

/// The tokens SEEK has used, over all the sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifetimeUsage {
    pub input: u32,
    pub output: u32,
}

impl LifetimeUsage {
    /// Read the usage of the state.
    pub fn read(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return LifetimeUsage::default();
        };
        let number = |key: &str| -> u32 {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .map(|value| value.trim_start_matches([' ', '=']))
                .and_then(|value| value.parse().ok())
                .unwrap_or(0)
        };
        LifetimeUsage {
            input: number("input"),
            output: number("output"),
        }
    }

    /// Write the usage to the state.
    pub fn write(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            format!("input = {}\noutput = {}\n", self.input, self.output),
        )
    }
}

/// The folder with the built-in widgets.
pub fn builtin_widgets() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("SEEK_BUILTIN") {
        return std::path::PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(folder) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
    {
        let candidate = folder.join("builtin");
        if candidate.is_dir() {
            return candidate;
        }
    }
    std::path::PathBuf::from("builtin")
}

/// The name of a local model, without the `local/` prefix.
pub fn local_model_name(id: &str) -> String {
    id.strip_prefix("local/").unwrap_or(id).to_string()
}

/// Read `~/seek/providers.toml`, if it is there.
///
/// A bare install has no file, so the providers are detected from the environment: the common
/// keys are looked for and a provider is made for every one that is set (SPEC 6).
pub fn load_providers(home: &Path) -> ProvidersConfig {
    let path = home.join(seek_core::config::PROVIDERS_FILE);
    let config = ProvidersConfig::read(&path).unwrap_or_default();
    if config.provider.is_empty() {
        let detected = detect_providers();
        if !detected.provider.is_empty() {
            return detected;
        }
    }
    config
}

/// The providers that the environment points at.
pub fn detect_providers() -> ProvidersConfig {
    let known = [
        (
            "anthropic",
            "ANTHROPIC_API_KEY",
            "anthropic-messages",
            "https://api.anthropic.com",
            "claude-sonnet-4-5",
        ),
        (
            "openai",
            "OPENAI_API_KEY",
            "openai-chat",
            "https://api.openai.com/v1",
            "gpt-5.4",
        ),
        (
            "openrouter",
            "OPENROUTER_API_KEY",
            "openai-chat",
            "https://openrouter.ai/api/v1",
            "anthropic/claude-sonnet-4.5",
        ),
    ];

    let mut config = ProvidersConfig::default();
    for (id, variable, kind, base_url, model) in known {
        match std::env::var(variable) {
            Ok(value) if !value.is_empty() => {}
            _ => continue,
        }
        let mut models = std::collections::BTreeMap::new();
        models.insert(
            model.to_string(),
            seek_core::ModelConfig {
                name: None,
                context: None,
                rpm: None,
                tpm: None,
                variant_default: None,
                variants: std::collections::BTreeMap::new(),
                disabled: None,
            },
        );
        config.provider.insert(
            id.to_string(),
            seek_core::ProviderConfig {
                name: Some(id.to_string()),
                kind: Some(kind.to_string()),
                base_url: Some(base_url.to_string()),
                env: vec![variable.to_string()],
                models,
            },
        );
    }
    config
}

/// The llama.cpp binary and the subcommand that starts the server.
///
/// The spec says `llama-server`, but the shipped llama.cpp since 0.9.0 is one `llama` binary
/// whose server is the `serve` subcommand (DECISIONS.md D-001). The config points at the binary
/// and at the subcommand; the legacy binary has no subcommand at all.
pub fn find_llama(home: &Path) -> (String, Option<String>) {
    if let Ok(config) = seek_core::Config::read(&home.join(seek_core::config::CONFIG_FILE)) {
        let (path, subcommand) = config.llama_command();
        if config.llama_path.is_some() {
            return (path, subcommand);
        }
    }
    if which("llama").is_some() {
        return ("llama".to_string(), Some("serve".to_string()));
    }
    if which("llama-server").is_some() {
        // The legacy binary is the server itself.
        return ("llama-server".to_string(), None);
    }
    ("llama".to_string(), Some("serve".to_string()))
}

/// Read the configuration and migrate it, if it is old.
pub fn load_config(home: &Path) -> seek_core::Config {
    let path = home.join(seek_core::config::CONFIG_FILE);
    let Ok(mut config) = seek_core::Config::read(&path) else {
        return seek_core::Config::default();
    };
    if config.migrate() {
        let _ = config.write(&path);
    }
    config
}

/// The path of a program on `PATH`.
pub fn which(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The current git branch, if there is one.
pub fn git_branch() -> Option<String> {
    let output = ProcessCommand::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?;
    let branch = branch.trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

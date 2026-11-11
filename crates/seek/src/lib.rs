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
}

impl Runtime {
    /// The runtime for a SEEK home.
    pub fn new(home: &Path, binary: &str) -> Self {
        let models_dir = home.join(seek_local::MODELS_DIR);
        let local_models = seek_local::model::scan(&models_dir);
        let supervisor =
            Supervisor::new(binary.to_string(), seek_local::supervisor::state_dir(home));
        // A server of a previous run does not survive SEEK.
        supervisor.kill_orphans();
        Runtime {
            supervisor: tokio::sync::Mutex::new(supervisor),
            local_models,
            current_model: Mutex::new(None),
        }
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
            }
            Command::Submit { text, mode } => {
                let tools = ToolRegistry {
                    read: Some(ReadTool::new(root.clone())),
                    write: Some(WriteTool::new(root.clone())),
                    edit: Some(EditTool::new(root.clone())),
                    bash: Some(BashTool::new(root.clone(), processes.clone())),
                };

                let mut history = History::new();
                history.push(seek_core::Message::user(text));

                let provider = match runtime.current_model() {
                    Some(id) if id.starts_with("local/") => {
                        match runtime.ensure_local_server(&id).await {
                            Ok(port) => Some(Box::new(OpenAiProvider::new(
                                format!("http://127.0.0.1:{port}/v1"),
                                None,
                                local_model_name(&id),
                                8_192,
                            )) as Box<dyn Provider>),
                            Err(error) => {
                                let _ = events.send(AgentEvent::Error(error)).await;
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
                        );
                        agent.run_turn(&mut history, &events).await;
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

/// The name of a local model, without the `local/` prefix.
pub fn local_model_name(id: &str) -> String {
    id.strip_prefix("local/").unwrap_or(id).to_string()
}

/// Read `~/seek/providers.toml`, if it is there.
pub fn load_providers(home: &Path) -> ProvidersConfig {
    let path = home.join(seek_core::config::PROVIDERS_FILE);
    ProvidersConfig::read(&path).unwrap_or_default()
}

/// The llama.cpp binary: the one in the config, else `llama` on the PATH.
///
/// The spec still says `llama-server`, but the shipped llama.cpp since 0.9.0 is one `llama`
/// binary whose server is the `serve` subcommand (DECISIONS.md D-001).
pub fn find_llama(home: &Path) -> String {
    if let Ok(config) = seek_core::Config::read(&home.join(seek_core::config::CONFIG_FILE))
        && let Some(path) = config.llama_server_path
    {
        return path;
    }
    if which("llama").is_some() {
        return "llama".to_string();
    }
    if which("llama-server").is_some() {
        return "llama-server".to_string();
    }
    "llama".to_string()
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

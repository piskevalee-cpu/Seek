//! The SEEK binary: wiring, CLI and the doctor (SPEC 4.1).

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::Result;
use clap::Parser;
use seek_core::{AgentEvent, ProvidersConfig};
use seek_llm::FakeProvider;
use seek_llm::anthropic::AnthropicProvider;
use seek_llm::openai::OpenAiProvider;
use seek_llm::provider::{ChatRequest, Provider, ProviderEvent};
use seek_tui::app::{App, Command};
use seek_tui::run::{init_terminal, restore_terminal, run};
use seek_tui::theme::Theme;
use tokio::sync::mpsc;

/// The version, with the short git SHA (SPEC 2.4).
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("SEEK_GIT_SHA"), ")");

/// SEEK: a thin, local-first coding-agent harness for the terminal.
#[derive(Debug, Parser)]
#[command(name = "seek", version = VERSION, about, long_about = None)]
struct Cli {
    /// Reduce motion: replace the spinners with static text.
    #[arg(long, global = true)]
    reduce_motion: bool,

    /// Print the configuration and exit.
    #[arg(long, global = true)]
    doctor: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let home = seek_core::seek_home();
    let providers = load_providers(&home);

    if cli.doctor {
        return doctor(&home, &providers);
    }

    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);

    // The agent task. It talks to the real provider when one is configured, and to the scripted
    // provider otherwise.
    tokio::spawn(agent_loop(command_rx, event_tx, providers.clone()));

    let mut app = App::new(Theme::for_terminal());
    app.cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    app.branch = git_branch();
    app.reduce_motion = cli.reduce_motion;
    app.providers = providers;

    // `ratatui::init` installs a panic hook that restores the terminal.
    let terminal = init_terminal();
    let result = run(terminal, app, event_rx, command_tx).await;
    restore_terminal();

    result
}

/// Read `~/seek/providers.toml`, if it is there.
fn load_providers(home: &Path) -> ProvidersConfig {
    let path = home.join(seek_core::config::PROVIDERS_FILE);
    ProvidersConfig::read(&path).unwrap_or_default()
}

/// Report what SEEK found (SPEC 13).
fn doctor(home: &Path, providers: &ProvidersConfig) -> Result<()> {
    println!("SEEK {}", VERSION);
    println!();
    println!("home            {}", home.display());
    println!(
        "providers.toml  {}",
        if home.join(seek_core::config::PROVIDERS_FILE).exists() {
            "found"
        } else {
            "missing"
        }
    );

    let mut configured = 0;
    for (id, provider) in &providers.provider {
        let key = provider.api_key();
        let state = if key.is_some() { "key found" } else { "no key" };
        println!(
            "  {id:<16} {:<20} {state}",
            provider.kind.clone().unwrap_or_default()
        );
        configured += 1;
    }
    if configured == 0 {
        println!("  no providers configured");
    }

    println!();
    let git = which("git");
    println!(
        "git             {}",
        git.map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".to_string())
    );
    let llama = which("llama").or_else(|| which("llama-server"));
    println!(
        "llama server    {}",
        llama
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".to_string())
    );

    Ok(())
}

/// The path of a program on `PATH`.
fn which(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run the agent: every submitted message becomes one turn.
async fn agent_loop(
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<AgentEvent>,
    providers: ProvidersConfig,
) {
    use futures::StreamExt;

    while let Some(command) = commands.recv().await {
        match command {
            Command::Quit => break,
            Command::Submit(text) => {
                if events.send(AgentEvent::TurnStarted).await.is_err() {
                    break;
                }
                let request = ChatRequest {
                    messages: vec![seek_core::Message::user(text)],
                    ..ChatRequest::default()
                };

                match build_provider(&providers) {
                    Some(provider) => {
                        let mut stream = provider.stream(request);
                        while let Some(event) = stream.next().await {
                            let Some(event) = event.ok() else { break };
                            let agent_event = match event {
                                ProviderEvent::ThinkingDelta(text) => {
                                    AgentEvent::ThinkingDelta(text)
                                }
                                ProviderEvent::TextDelta(text) => AgentEvent::TextDelta(text),
                                ProviderEvent::ToolCall { id, name, input } => {
                                    AgentEvent::ToolCallReady { id, name, input }
                                }
                                ProviderEvent::Usage { input, output } => {
                                    AgentEvent::Usage(seek_core::Usage { input, output })
                                }
                                ProviderEvent::Done => {
                                    AgentEvent::TurnFinished(seek_core::TurnReason::Stop)
                                }
                            };
                            if events.send(agent_event).await.is_err() {
                                return;
                            }
                        }
                    }
                    None => {
                        // No provider is configured: the scripted provider keeps the UI alive.
                        let provider = FakeProvider::demo();
                        let mut stream = provider.stream(request);
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

/// Build a provider from the configuration.
///
/// The first provider that has a key in the environment wins. Without one, SEEK runs on the
/// scripted provider.
fn build_provider(providers: &ProvidersConfig) -> Option<Box<dyn Provider>> {
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

/// The current git branch, if there is one.
fn git_branch() -> Option<String> {
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

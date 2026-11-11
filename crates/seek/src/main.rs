//! The SEEK binary: wiring, CLI and the doctor (SPEC 4.1).

use std::process::Command as ProcessCommand;

use anyhow::Result;
use clap::Parser;
use seek_core::AgentEvent;
use seek_llm::{ChatRequest, FakeProvider, Provider, ProviderEvent};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);

    // The agent task. Milestone v0.1 runs a scripted provider; the real one lands in v0.2.
    tokio::spawn(agent_loop(command_rx, event_tx));

    let mut app = App::new(Theme::for_terminal());
    app.cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    app.branch = git_branch();
    app.reduce_motion = cli.reduce_motion;

    // `ratatui::init` installs a panic hook that restores the terminal.
    let terminal = init_terminal();
    let result = run(terminal, app, event_rx, command_tx).await;
    restore_terminal();

    result
}

/// Run the fake agent: turn each submitted message into a scripted turn.
async fn agent_loop(mut commands: mpsc::Receiver<Command>, events: mpsc::Sender<AgentEvent>) {
    let provider = FakeProvider::demo();
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
                let mut stream = provider.stream(request);
                use futures::StreamExt;
                while let Some(event) = stream.next().await {
                    let Some(event) = event.ok() else { break };
                    let agent_event = match event {
                        ProviderEvent::ThinkingDelta(text) => AgentEvent::ThinkingDelta(text),
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
        }
    }
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

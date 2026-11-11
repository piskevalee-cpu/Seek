//! The SEEK binary: the command line and the wiring (SPEC 4.1).

use anyhow::Result;
use clap::Parser;
use seek::doctor::doctor;
use seek_core::AgentEvent;
use seek_core::r#loop::PermissionRequests;
use seek_tui::app::{App, Command};
use seek_tui::run::{init_terminal, restore_terminal, run};
use seek_tui::theme::Theme;
use std::sync::Arc;
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
    let providers = seek::load_providers(&home);

    if cli.doctor {
        return doctor(&home, &providers);
    }

    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
    let (permission_tx, mut permission_rx) = PermissionRequests::new();

    // The permission requests of the agent are forwarded to the UI.
    let permission_events = event_tx.clone();
    tokio::spawn(async move {
        while let Some(request) = permission_rx.recv().await {
            if permission_events
                .send(AgentEvent::PermissionRequest(request))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // The local models of ~/seek/models, and the supervisor of the local server.
    let llama_binary = seek::find_llama(&home);
    let runtime = Arc::new(seek::Runtime::new(&home, &llama_binary));
    let logs = runtime.supervisor.lock().await.logs_handle();

    // The agent task. It talks to the real provider when one is configured, and to the scripted
    // provider otherwise.
    tokio::spawn(seek::agent_loop(
        command_rx,
        event_tx,
        providers.clone(),
        permission_tx.clone(),
        Arc::clone(&runtime),
    ));

    let mut app = App::new(Theme::for_terminal());
    app.cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    app.branch = seek::git_branch();
    app.reduce_motion = cli.reduce_motion;
    app.providers = providers;
    app.processes = Some(Arc::new(seek_core::ProcessRegistry::new()));
    app.local_models = runtime.local_models.clone();
    app.logs = Some(logs);
    app.skills = runtime.skills.clone();
    app.toggles = runtime.toggles.clone();
    app.external_tools = runtime.external_tools.clone();

    // `ratatui::init` installs a panic hook that restores the terminal.
    let terminal = init_terminal();
    let result = run(terminal, app, event_rx, command_tx).await;
    restore_terminal();

    result
}

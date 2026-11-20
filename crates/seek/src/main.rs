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

    /// Resume the last session of this directory, with its messages and model.
    #[arg(long, global = true)]
    continue_: bool,

    #[command(subcommand)]
    action: Option<Action>,
}

/// Commands that do their work without opening the TUI.
#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Update SEEK to the latest release of main.
    Update {
        /// Only report whether a newer release exists.
        #[arg(long)]
        check: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Action::Update { check }) = cli.action {
        return seek::update::run(check).await;
    }

    let home = seek_core::seek_home();
    let providers = seek::load_providers(&home);

    if cli.doctor {
        return doctor(&home, &providers);
    }

    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
    let (widget_tx, widget_rx) = mpsc::channel::<seek_tui::app::UiMessage>(16);
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
    let (llama_binary, llama_subcommand) = seek::find_llama(&home);
    let runtime = Arc::new(seek::Runtime::new(
        &home,
        &llama_binary,
        llama_subcommand.clone(),
    ));
    let logs = runtime.supervisor.lock().await.logs_handle();

    // `--continue` replays the last session of this directory before the agent loop starts, so the
    // model sees the same history and the UI shows it on the first frame (SPEC 11).
    let mut continued_messages: Option<Vec<seek_core::Message>> = None;
    let mut continued_model: Option<String> = None;
    if cli.continue_
        && let Some(id) = runtime.latest_session()
    {
        match runtime.resume_session(&id).await {
            Ok(history) => {
                continued_messages = Some(history.messages);
                continued_model = runtime.session_model(&id);
                if let Some(model) = &continued_model {
                    runtime.set_current_model(Some(model.clone()));
                }
            }
            Err(error) => eprintln!("could not resume {id}: {error}"),
        }
    }

    // The agent task. It talks to the real provider when one is configured, and to the scripted
    // provider otherwise.
    tokio::spawn(seek::agent_loop(
        command_rx,
        event_tx,
        widget_tx.clone(),
        providers.clone(),
        permission_tx.clone(),
        Arc::clone(&runtime),
    ));

    // The models the endpoints know are fetched once at the start (SPEC 6).
    {
        let providers = providers.clone();
        let widget_tx = widget_tx.clone();
        tokio::spawn(async move {
            let models = seek::discover_models(&providers).await;
            if !models.is_empty() {
                let _ = widget_tx
                    .send(seek_tui::app::UiMessage::Models(models))
                    .await;
            }
        });
    }

    // A signal stops the servers before SEEK leaves: the destructors do not run when the
    // process is killed, and a local server would stay behind (SPEC 7.3).
    {
        let runtime = Arc::clone(&runtime);
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let (Ok(mut terminate), Ok(mut interrupt), Ok(mut hangup)) = (
                signal(SignalKind::terminate()),
                signal(SignalKind::interrupt()),
                signal(SignalKind::hangup()),
            ) else {
                return;
            };
            tokio::select! {
                _ = terminate.recv() => {}
                _ = interrupt.recv() => {}
                _ = hangup.recv() => {}
            }
            runtime.stop_servers().await;
            let user = runtime.user.lock().await;
            let _ = user.write(&runtime.state_dir.join(seek_core::user::USER_FILE));
            std::process::exit(0);
        });
    }

    let mut app = App::new(Theme::for_terminal());
    // The default permission level is a startup setting: the footer shows it from the first frame.
    app.mode = runtime
        .config
        .default_mode
        .as_deref()
        .and_then(seek_core::PermissionMode::parse)
        .unwrap_or(seek_core::PermissionMode::Ask);
    app.cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    app.branch = seek::git_branch();
    app.reduce_motion = cli.reduce_motion;
    app.providers = providers;
    app.processes = Some(runtime.processes.clone());
    app.local_models = runtime.local_models.clone();
    app.logs = Some(logs);
    app.skills = runtime.skills.clone();
    // The name of the user: the first run asks for it (SPEC 9.5).
    app.user_name = Some(runtime.user.lock().await.name.clone());
    app.sidebar_side = runtime.user.lock().await.sidebar_side;
    app.sidebar_blocks = runtime.user.lock().await.sidebar_blocks.clone();
    app.sidebar.open = runtime.user.lock().await.sidebar_open;
    app.toggles = runtime
        .toggles
        .read()
        .expect("toggles are not poisoned")
        .clone();
    app.external_tools = runtime.external_tools.clone();
    app.widgets = runtime.widgets.clone();
    app.llama_subcommand = llama_subcommand;

    if let Some(messages) = continued_messages {
        app.update(seek_tui::app::Message::Session(messages));
    }
    if let Some(id) = continued_model {
        if id.starts_with("local/") {
            app.select_local_model(&id);
        } else {
            app.select_model(&id);
        }
        app.model_configured = true;
    }

    // `ratatui::init` installs a panic hook that restores the terminal.
    let terminal = init_terminal();
    let result = run(terminal, app, event_rx, widget_rx, command_tx).await;
    restore_terminal();

    result
}

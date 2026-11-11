//! The event loop (SPEC 4.2).
//!
//! One `tokio::select!` reads the crossterm `EventStream`, a 100 ms tick and the agent's events.
//! The screen is redrawn only when the state changed or an animation is running.

use std::io::Stdout;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use seek_core::AgentEvent;
use tokio::sync::mpsc;

use crate::app::{App, Command, Message, TICK_MS};

/// The terminal type SEEK draws on.
pub type SeekTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Set the terminal up for drawing.
///
/// `ratatui::init` installs a panic hook that restores the terminal, so a crash never leaves the
/// terminal in a broken state (SPEC 4.2).
pub fn init_terminal() -> SeekTerminal {
    ratatui::init()
}

/// Put the terminal back the way it was.
pub fn restore_terminal() {
    ratatui::restore();
}

/// Run the UI until the user quits.
///
/// `agent_events` carries the events of the running agent; `commands` receives the side effects
/// the UI wants performed.
pub async fn run(
    mut terminal: SeekTerminal,
    mut app: App,
    mut agent_events: mpsc::Receiver<AgentEvent>,
    mut widget_events: mpsc::Receiver<seek_core::WidgetOutput>,
    commands: mpsc::Sender<Command>,
) -> anyhow::Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // The application starts: the first run asks which model to use.
    app.update(Message::Start);

    loop {
        terminal.draw(|frame| app.view(frame))?;

        tokio::select! {
            Some(Ok(event)) = events.next() => {
                let message = match event {
                    Event::Key(key) if key.kind == KeyEventKind::Press => Some(Message::Key(key)),
                    Event::Key(_) => None,
                    Event::Resize(width, height) => Some(Message::Resize(width, height)),
                    _ => None,
                };
                if let Some(message) = message
                    && let Some(command) = app.update(message)
                    && commands.send(command).await.is_err()
                {
                    break;
                }
            }
            Some(event) = agent_events.recv() => {
                if let Some(command) = app.update(Message::Agent(event))
                    && commands.send(command).await.is_err()
                {
                    break;
                }
            }
            Some(output) = widget_events.recv() => {
                if let Some(command) = app.update(Message::Widget(output))
                    && commands.send(command).await.is_err()
                {
                    break;
                }
            }
            _ = ticker.tick() => {
                app.update(Message::Tick);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

//! The event loop (SPEC 4.2).
//!
//! One `tokio::select!` reads the keys, a 100 ms tick and the agent's events.
//! The screen is redrawn only when the state changed or an animation is running.

use std::io::Stdout;
use std::time::Duration;

use crossterm::event::{Event, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use seek_core::AgentEvent;
use tokio::sync::mpsc;

use crate::app::{App, Command, Message, TICK_MS};

/// How many lines the wheel scrolls at once.
pub const MOUSE_SCROLL: isize = 3;

/// The terminal type SEEK draws on.
pub type SeekTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Set the terminal up for drawing.
///
/// `ratatui::init` installs a panic hook that restores the terminal, so a crash never leaves the
/// terminal in a broken state (SPEC 4.2). The mouse is taken as well: the wheel scrolls
/// the transcript, the way a chat does.
pub fn init_terminal() -> SeekTerminal {
    use crossterm::event::EnableMouseCapture;
    let _ = EnableMouseCapture;
    ratatui::init()
}

/// Put the terminal back the way it was.
pub fn restore_terminal() {
    use crossterm::event::DisableMouseCapture;
    let _ = DisableMouseCapture;
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
    mut ui_events: mpsc::Receiver<crate::app::UiMessage>,
    commands: mpsc::Sender<Command>,
) -> anyhow::Result<()> {
    // The keys come from a thread of their own. The reader of crossterm blocks until
    // something happens, and a blocked reader inside the event loop freezes the whole
    // interface: the keys arrive late, or not at all.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if event_tx.send(event).is_err() {
                break;
            }
        }
    });

    let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // The application starts: the first run asks which model to use.
    app.update(Message::Start);

    loop {
        terminal.draw(|frame| app.view(frame))?;

        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else { break };
                let message = match event {
                    Event::Key(key) if key.kind == KeyEventKind::Press => Some(Message::Key(key)),
                    Event::Key(_) => None,
                    Event::Resize(width, height) => Some(Message::Resize(width, height)),
                    Event::Mouse(mouse) => match mouse.kind {
                        crossterm::event::MouseEventKind::ScrollUp => {
                            Some(Message::Scroll(MOUSE_SCROLL))
                        }
                        crossterm::event::MouseEventKind::ScrollDown => {
                            Some(Message::Scroll(-MOUSE_SCROLL))
                        }
                        _ => None,
                    },
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
            Some(message) = ui_events.recv() => {
                let message = match message {
                    crate::app::UiMessage::Widget(output) => Message::Widget(output),
                    crate::app::UiMessage::Context(total) => Message::Context(total),
                    crate::app::UiMessage::Sessions(sessions) => Message::Sessions(sessions),
                    crate::app::UiMessage::Session(messages) => Message::Session(messages),
                    crate::app::UiMessage::Models(models) => Message::Models(models),
                };
                if let Some(command) = app.update(message)
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

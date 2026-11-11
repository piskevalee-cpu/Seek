//! UI tests with the ratatui `TestBackend` (SPEC 14).
//!
//! These tests render the real application into a buffer and check what ends up on the screen:
//! the Home screen, the Chat layout, the sidebar, the permission ask and the ownership table.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use seek_core::AgentEvent;
use seek_tui::app::{App, Message, Overlay};
use seek_tui::theme::Theme;
use seek_tui::transcript::Row;

/// A key press.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Render the app into a buffer and return the screen as text.
fn render(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<Vec<_>>()
        .join("")
}

/// Render with line breaks, so that assertions can look at whole lines.
fn render_lines(app: &App, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    let mut lines = Vec::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer.content[(y * area.width + x) as usize].symbol());
        }
        lines.push(line);
    }
    lines
}

fn app() -> App {
    let mut app = App::new(Theme::default());
    app.cwd = "/tmp".into();
    app.branch = Some("develop".into());
    app
}

#[test]
fn the_home_screen_shows_the_logo() {
    let app = app();
    let screen = render(&app, 100, 30);
    // The full-block logo is 50 columns wide and 9 rows tall.
    assert!(screen.contains("██"), "the logo is not on the home screen");
}

#[test]
fn a_small_terminal_says_so() {
    let app = app();
    let screen = render(&app, 40, 10);
    assert!(screen.contains("terminal too small"));
}

#[test]
fn the_chat_layout_shows_the_message_the_footer_and_the_prompt() {
    let mut app = app();
    app.rows.push(Row::User("hello".into()));
    app.rows.push(Row::Assistant("hi there".into()));
    let lines = render_lines(&app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("hello")));
    assert!(lines.iter().any(|line| line.contains("hi there")));
    // The footer chip and the prompt marker.
    assert!(lines.iter().any(|line| line.contains("ASK")));
    assert!(lines.iter().any(|line| line.contains(">")));
}

#[test]
fn the_sidebar_can_be_shown() {
    let mut app = app();
    app.sidebar.open = true;
    let screen = render(&app, 120, 30);
    assert!(screen.contains("Sessions"));
    assert!(screen.contains("Processes"));
}

#[test]
fn the_sidebar_is_hidden_on_a_narrow_terminal() {
    let mut app = app();
    app.sidebar.open = true;
    let screen = render(&app, 90, 30);
    assert!(!screen.contains("Sessions"));
}

#[test]
fn the_mode_chip_is_the_only_place_the_mode_is_shown() {
    let mut app = app();
    app.mode = seek_tui::PermissionMode::Yolo;
    let screen = render(&app, 100, 30);
    assert_eq!(screen.matches("YOLO").count(), 1);
}

#[test]
fn the_model_is_shown_in_the_footer() {
    let mut app = app();
    app.model = seek_tui::ModelChip {
        provider: "anthropic".into(),
        model: "claude-sonnet".into(),
        local: false,
    };
    let screen = render(&app, 100, 30);
    assert!(screen.contains("claude-sonnet"));
    assert!(screen.contains("anthropic"));
}

#[test]
fn the_local_tag_only_shows_for_local_models() {
    let mut app = app();
    app.model.local = true;
    let screen = render(&app, 100, 30);
    assert!(screen.contains("LOCAL"));
}

#[test]
fn reasoning_is_shown_and_hidden() {
    let mut app = app();
    app.rows.push(Row::Reasoning("secret thinking".into()));
    let shown = render(&app, 100, 30);
    assert!(shown.contains("secret thinking"));

    app.show_reasoning = false;
    let hidden = render(&app, 100, 30);
    assert!(!hidden.contains("secret thinking"));
    assert!(hidden.contains("reasoning"));
}

#[test]
fn a_queued_message_is_tagged() {
    let mut app = app();
    app.rows.push(Row::Pending("later".into()));
    let screen = render(&app, 100, 30);
    assert!(screen.contains("queued"));
}

#[test]
fn a_tool_row_shows_the_command_and_the_status() {
    let mut app = app();
    app.rows.push(Row::Tool {
        name: "bash".into(),
        command: "cargo test".into(),
        status: "ok".into(),
        duration: Some(std::time::Duration::from_millis(1200)),
        detail: Some("all good".into()),
        expanded: false,
    });
    let screen = render(&app, 100, 30);
    assert!(screen.contains("cargo test"));
    assert!(screen.contains("ok"));
    assert!(!screen.contains("all good"));
}

#[test]
fn the_help_overlay_lists_the_keys() {
    let mut app = app();
    app.overlay = Some(Overlay::Help);
    let screen = render(&app, 100, 30);
    assert!(screen.contains("Ctrl+B"));
    assert!(screen.contains("Alt+T"));
}

#[test]
fn the_spinner_appears_while_the_agent_thinks() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::TurnStarted));
    let screen = render(&app, 100, 30);
    assert!(screen.contains("██"));
}

#[test]
fn the_ownership_table_holds_for_the_regions_that_exist() {
    // Every fact has exactly one home (SPEC 10.2): the mode chip only in the footer, the model
    // only in the footer, the reasoning text only in the transcript.
    let mut app = app();
    app.rows.push(Row::Reasoning("private".into()));
    app.model = seek_tui::ModelChip {
        provider: "fake".into(),
        model: "fake/demo".into(),
        local: false,
    };

    let lines = render_lines(&app, 100, 30);
    let footer = lines.last().expect("there is a footer");

    assert!(
        footer.contains("ASK"),
        "the mode chip belongs in the footer"
    );
    assert!(
        footer.contains("fake/demo"),
        "the model belongs in the footer"
    );

    let transcript: String = lines[..lines.len() - 1].join("\n");
    assert!(
        transcript.contains("private"),
        "reasoning belongs in the transcript"
    );
    assert!(
        !transcript.contains("ASK"),
        "the mode must not be shown outside the footer"
    );
}

#[test]
fn the_model_picker_lists_the_catalog() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&app, 100, 30);
    assert!(screen.contains("Claude Sonnet 4.5"));
    assert!(screen.contains("GPT-5.4"));
}

#[test]
fn choosing_a_model_shows_it_in_the_footer() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));
    // Enter takes the first model.
    app.update(Message::Key(key(KeyCode::Enter)));

    let lines = render_lines(&app, 100, 30);
    let footer = lines.last().expect("there is a footer");
    assert!(
        footer.contains("Claude Sonnet 4.5"),
        "the model is in the footer"
    );
}

#[test]
fn the_provider_picker_is_empty_without_configuration() {
    let mut app = app();
    for c in "/provider".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&app, 100, 30);
    assert!(screen.contains("Add a provider"));
}

#[test]
fn the_picker_can_be_filtered_and_closed() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    for c in "gpt".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let screen = render(&app, 100, 30);
    assert!(screen.contains("GPT-5.4"));
    assert!(!screen.contains("Claude Sonnet 4.5"));

    app.update(Message::Key(key(KeyCode::Esc)));
    let screen = render(&app, 100, 30);
    assert!(!screen.contains("GPT-5.4"));
}

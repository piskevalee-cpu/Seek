//! Render the UI into a text buffer and print it.
//!
//! Useful for checking the layout without a terminal:
//!
//! ```sh
//! cargo run -p seek-tui --example screenshot
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use seek_tui::app;
use seek_tui::app::App;
use seek_tui::theme::Theme;
use seek_tui::transcript::Row;

/// Render the app and return the screen, line by line.
fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer.content[(y * area.width + x) as usize].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn main() {
    let mut app = App::new(Theme::default());
    app.cwd = "/home/user/project".into();
    app.branch = Some("develop".into());

    println!("== HOME ==");
    for line in screen(&mut app, 100, 28) {
        println!("{line}");
    }

    app.rows.push(Row::User("add a test for the parser".into()));
    app.rows
        .push(Row::Reasoning("the parser lives in seek-core".into()));
    app.rows.push(Row::Assistant("Added a parser test.".into()));
    app.rows.push(Row::Tool {
        name: "bash".into(),
        command: "cargo test".into(),
        status: "ok".into(),
        duration: Some(std::time::Duration::from_millis(2400)),
        detail: Some("test result: ok. 3 passed".into()),
        expanded: false,
    });

    println!("\n== CHAT ==");
    for line in screen(&mut app, 100, 28) {
        println!("{line}");
    }

    app.sidebar.open = true;
    println!("\n== CHAT, SIDEBAR ==");
    for line in screen(&mut app, 120, 28) {
        println!("{line}");
    }

    // The model picker.
    let mut picker = app::App::new(Theme::default());
    for c in "/model".chars() {
        picker.update(app::Message::Key(key(KeyCode::Char(c))));
    }
    picker.update(app::Message::Key(key(KeyCode::Enter)));
    println!("\n== MODEL PICKER ==");
    for line in screen(&mut picker, 100, 28) {
        println!("{line}");
    }
}

/// A key press.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

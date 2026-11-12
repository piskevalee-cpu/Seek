//! Dump a screen of the interface, so it can be looked at as a picture.
//!
//! ```sh
//! cargo run -p seek-tui --example dump_screen -- home > /tmp/home.json
//! python3 tools/screen_to_png.py /tmp/home.json /tmp/home.png
//! ```
//!
//! The dump is the cells of the buffer with their colours.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use seek_core::AgentEvent;
use seek_tui::app::{App, Message};
use seek_tui::theme::Theme;
use seek_tui::transcript::Row;

/// The colour of a cell, as the dump writes it.
fn color(value: Option<Color>) -> String {
    match value {
        Some(Color::Rgb(r, g, b)) => format!("#{r:02X}{g:02X}{b:02X}"),
        Some(Color::Indexed(i)) => format!("i{i}"),
        Some(Color::Reset) => "reset".to_string(),
        Some(other) => format!("{other:?}"),
        None => "none".to_string(),
    }
}

fn main() {
    let screen = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "home".to_string());
    let (width, height) = (100u16, 30u16);

    let mut app = App::new(Theme::default());
    app.cwd = "/home/valerio/projects/seek-harness".into();
    app.branch = Some("develop".into());
    app.model = seek_tui::ModelChip {
        provider: "local".into(),
        model: "MiniCPM5-2B-heretic".into(),
        local: true,
    };

    match screen.as_str() {
        "chat" => {
            app.rows
                .push(Row::User("which files are in this directory?".into()));
            app.rows.push(Row::Reasoning(
                "The user wants to know what is in the directory. I should run ls.".into(),
            ));
            app.rows
                .push(Row::Assistant("Let me look at the directory.".into()));
            app.rows.push(Row::Tool {
                id: "t1".to_string(),
                name: "bash".into(),
                command: "ls -la".into(),
                status: "ok".into(),
                duration: Some(std::time::Duration::from_millis(240)),
                detail: Some("Cargo.toml  crates  docs  target".into()),
                expanded: false,
            });
            app.rows.push(Row::Assistant(
                "There are eight entries: Cargo.toml, crates, docs, target and four more.".into(),
            ));
        }
        "permission" => {
            app.rows.push(Row::User("read Cargo.toml".into()));
            app.update(Message::Agent(AgentEvent::PermissionRequest(
                seek_core::PermissionRequest {
                    id: 1,
                    tool: "bash".into(),
                    summary: "bash ls -la".into(),
                },
            )));
        }
        _ => {
            app.widget_outputs = vec![
                seek_core::WidgetOutput {
                    name: "git".into(),
                    title: "recent commits".into(),
                    lines: vec![
                        "on develop".into(),
                        "a1b2c3d feat: the shimmer line".into(),
                        "e4f5g6h fix: the transcript window".into(),
                    ],
                    failed: false,
                },
                seek_core::WidgetOutput {
                    name: "usage".into(),
                    title: "tokens used".into(),
                    lines: vec!["input  412k".into(), "output 38k".into()],
                    failed: false,
                },
            ];
        }
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();

    let mut cells = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer.content[(y * width + x) as usize];
            let style = cell.style();
            cells.push(serde_json::json!({
                "c": cell.symbol(),
                "fg": color(style.fg),
                "bg": color(style.bg),
            }));
        }
    }
    println!(
        "{}",
        serde_json::json!({ "width": width, "height": height, "cells": cells })
    );
}

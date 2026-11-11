//! The transcript (SPEC 10.3).
//!
//! User and assistant messages are dark cube boxes: a filled dark surface, a one-cell accent on
//! the left edge (gray for the user, green for the assistant) and a role label on the first line.
//! Reasoning is plain dim text with a thin vertical bar at the left and no box; when reasoning is
//! hidden it collapses to a single row with the same bar. Tool rows are one line and can be
//! expanded. Queued messages render dimmed with a `queued` tag.

use std::time::Duration;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

/// The surface of a message box.
pub const SURFACE: ratatui::style::Color = ratatui::style::Color::Rgb(0x18, 0x1A, 0x1B);

/// One row of the transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// A message from the user.
    User(String),
    /// A message from the assistant.
    Assistant(String),
    /// Reasoning text.
    Reasoning(String),
    /// A tool call.
    Tool {
        name: String,
        command: String,
        status: String,
        duration: Option<Duration>,
        detail: Option<String>,
        expanded: bool,
    },
    /// A turn-ending error.
    Error(String),
    /// A message that is queued until the current turn ends.
    Pending(String),
    /// The thinking indicator at the tail.
    Spinner,
}

/// The role label shown on the first line of a message box.
const USER_LABEL: &str = "USER";
const ASSISTANT_LABEL: &str = "SEEK";

/// Render the transcript into lines.
///
/// Every line carries the index of the row it belongs to, so the view can highlight the selected
/// row.
pub fn transcript_lines(
    rows: &[Row],
    theme: &Theme,
    show_reasoning: bool,
) -> Vec<(usize, Line<'static>)> {
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        match row {
            Row::User(text) => {
                lines.extend(
                    message_box(text, USER_LABEL, theme.cube_light, theme)
                        .into_iter()
                        .map(|line| (index, line)),
                );
            }
            Row::Assistant(text) => {
                lines.extend(
                    message_box(text, ASSISTANT_LABEL, theme.accent, theme)
                        .into_iter()
                        .map(|line| (index, line)),
                );
            }
            Row::Reasoning(text) => {
                if show_reasoning {
                    lines.extend(
                        reasoning_lines(text, theme)
                            .into_iter()
                            .map(|line| (index, line)),
                    );
                } else {
                    lines.push((index, reasoning_collapsed(theme)));
                }
            }
            Row::Tool {
                name,
                command,
                status,
                duration,
                detail,
                expanded,
            } => {
                lines.push((index, tool_line(name, command, status, *duration, theme)));
                if *expanded && let Some(detail) = detail {
                    for line in detail.lines() {
                        lines.push((
                            index,
                            Line::from(Span::styled(
                                format!("    {line}"),
                                Style::new().fg(theme.dim),
                            )),
                        ));
                    }
                }
            }
            Row::Error(text) => {
                lines.push((
                    index,
                    Line::from(vec![
                        Span::styled("! ", Style::new().fg(theme.error).bold()),
                        Span::styled(text.clone(), Style::new().fg(theme.error)),
                    ]),
                ));
            }
            Row::Pending(text) => {
                lines.push((
                    index,
                    Line::from(vec![
                        Span::styled("│ ", Style::new().fg(theme.dim)),
                        Span::styled(text.clone(), Style::new().fg(theme.dim).italic()),
                        Span::styled(" queued", Style::new().fg(theme.dim).italic()),
                    ]),
                ));
            }
            Row::Spinner => {}
        }
    }
    lines
}

/// A message box: accent edge, role label, filled surface.
fn message_box(
    text: &str,
    label: &str,
    accent: ratatui::style::Color,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let surface = Style::new().bg(SURFACE);
    let mut lines = Vec::new();
    lines.push(
        Line::from(vec![
            Span::styled("▌", Style::new().fg(accent)),
            Span::styled(
                format!(" {label}"),
                Style::new().fg(theme.dim).bold().bg(SURFACE),
            ),
        ])
        .style(surface),
    );
    for line in text.lines() {
        lines.push(
            Line::from(vec![
                Span::styled("▌", Style::new().fg(accent)),
                Span::styled(
                    format!(" {line}"),
                    Style::new().fg(theme.cube_light).bg(SURFACE),
                ),
            ])
            .style(surface),
        );
    }
    lines
}

/// Reasoning text: dim, with a thin vertical bar at the left.
fn reasoning_lines(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            Line::from(vec![
                Span::styled("│ ", Style::new().fg(theme.dim)),
                Span::styled(line.to_string(), Style::new().fg(theme.dim)),
            ])
        })
        .collect()
}

/// The collapsed reasoning row.
fn reasoning_collapsed(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("│ ", Style::new().fg(theme.dim)),
        Span::styled("reasoning", Style::new().fg(theme.dim).italic()),
    ])
}

/// One tool row: `> bash  cargo test  ok 1.2s`.
fn tool_line(
    name: &str,
    command: &str,
    status: &str,
    duration: Option<Duration>,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled("> ", Style::new().fg(theme.dim)),
        Span::styled(name.to_string(), Style::new().fg(theme.accent).bold()),
    ];
    if !command.is_empty() {
        spans.push(Span::styled(
            format!("  {command}"),
            Style::new().fg(theme.cube_light),
        ));
    }
    let status_color = match status {
        "ok" => theme.accent,
        "error" => theme.error,
        _ => theme.dim,
    };
    spans.push(Span::styled(
        format!("  {status}"),
        Style::new().fg(status_color),
    ));
    if let Some(duration) = duration {
        spans.push(Span::styled(
            format!(" {:.1}s", duration.as_secs_f64()),
            Style::new().fg(theme.dim),
        ));
    }
    Line::from(spans)
}

/// Greedy word wrap that respects wide characters.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if UnicodeWidthStr::width(candidate.as_str()) <= width {
            current = candidate;
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            // A single word that does not fit is broken up.
            let mut rest = word;
            while UnicodeWidthStr::width(rest) > width {
                let mut cut = 0;
                let mut used = 0;
                for (index, ch) in rest.char_indices() {
                    let char_width = UnicodeWidthStr::width(ch.to_string().as_str());
                    if used + char_width > width {
                        cut = index;
                        break;
                    }
                    used += char_width;
                    cut = index + ch.len_utf8();
                }
                if cut == 0 {
                    break;
                }
                lines.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            current = rest.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_message_has_an_accent_edge_and_a_label() {
        let theme = Theme::default();
        let lines = transcript_lines(&[Row::User("hi".into())], &theme, true);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, 0, "the line knows its row");
        assert!(lines[0].1.spans[0].content.contains("▌"));
        assert!(lines[0].1.to_string().contains(USER_LABEL));
    }

    #[test]
    fn hidden_reasoning_collapses_to_one_row() {
        let theme = Theme::default();
        let rows = [Row::Reasoning("secret".into())];
        let shown = transcript_lines(&rows, &theme, true);
        let hidden = transcript_lines(&rows, &theme, false);
        assert_eq!(shown.len(), 1);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].1.to_string().contains("reasoning"));
        assert!(!hidden[0].1.to_string().contains("secret"));
    }

    #[test]
    fn pending_rows_are_tagged() {
        let theme = Theme::default();
        let lines = transcript_lines(&[Row::Pending("later".into())], &theme, true);
        assert!(lines[0].1.to_string().contains("queued"));
    }

    #[test]
    fn tool_rows_are_one_line() {
        let theme = Theme::default();
        let rows = [Row::Tool {
            name: "bash".into(),
            command: "cargo test".into(),
            status: "ok".into(),
            duration: Some(Duration::from_millis(1200)),
            detail: None,
            expanded: false,
        }];
        let lines = transcript_lines(&rows, &theme, true);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].1.to_string().contains("cargo test"));
        assert!(lines[0].1.to_string().contains("1.2s"));
    }

    #[test]
    fn wrapping_respects_the_width() {
        let lines = wrap("one two three four five six", 10);
        assert!(
            lines
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 10)
        );
        assert_eq!(lines.join(" "), "one two three four five six");
    }

    #[test]
    fn a_long_word_is_broken_up() {
        let lines = wrap("abcdefghijklmno", 5);
        assert!(
            lines
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 5)
        );
        assert_eq!(lines.concat(), "abcdefghijklmno");
    }
}

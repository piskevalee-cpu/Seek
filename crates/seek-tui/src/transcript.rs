//! The transcript (SPEC 10.3).
//!
//! The look follows the grok-build pager (Apache-2.0): every block has a one-column rail on the
//! left, coloured by the kind of the block (blue for the user, magenta for the model, grey for
//! the reasoning and the tools), and a blank line between the blocks so they breathe.
//!
//! The message of the user is a band across the width with the arrow in front of it. The answer
//! The answer of the model is styled Markdown (bold, italic, code, headings, bullets, quotes).
//! Reasoning is plain dim text with a thin vertical bar at the left;
//! when reasoning is hidden it collapses to a single row with the same bar. Tool rows are one
//! line and can be expanded. Queued messages render dimmed with a `queued` tag.

use std::time::Duration;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

/// The rail of a block: one column, coloured by the kind of the block.
pub const RAIL: &str = "\u{2502} ";

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
        /// The id of the call; the output of the tool is matched on it.
        id: String,
        name: String,
        /// What the call does, in a few words: the intent of the model.
        intent: Option<String>,
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

/// The arrow in front of the prompt of the user.
const PROMPT_ARROW: &str = "\u{276F} ";

/// Render the transcript into lines.
///
/// Every line carries the index of the row it belongs to, so the view can highlight the selected
/// row.
pub fn transcript_lines(
    rows: &[Row],
    theme: &Theme,
    show_reasoning: bool,
    show_output: bool,
    width: usize,
) -> Vec<(usize, Line<'static>)> {
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        // A blank line between the blocks: the transcript must breathe.
        if !lines.is_empty() {
            lines.push((index, Line::from("")));
        }
        match row {
            Row::User(text) => {
                lines.extend(
                    user_lines(text, theme, width)
                        .into_iter()
                        .map(|line| (index, line)),
                );
            }
            Row::Assistant(text) => {
                lines.extend(
                    assistant_lines(text, theme, width)
                        .into_iter()
                        .map(|line| (index, line)),
                );
            }
            Row::Reasoning(text) => {
                if show_reasoning {
                    lines.extend(
                        reasoning_lines(text, theme, width)
                            .into_iter()
                            .map(|line| (index, line)),
                    );
                } else {
                    lines.push((index, reasoning_collapsed(theme)));
                }
            }
            Row::Tool {
                name,
                intent,
                command,
                status,
                duration,
                detail,
                expanded,
                ..
            } => {
                lines.push((
                    index,
                    tool_line(name, intent.as_deref(), command, status, *duration, theme),
                ));
                if show_output
                    && *expanded
                    && let Some(detail) = detail
                {
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
                        Span::styled(RAIL, Style::new().fg(theme.error)),
                        Span::styled("! ", Style::new().fg(theme.error).bold()),
                        Span::styled(text.clone(), Style::new().fg(theme.error)),
                    ]),
                ));
            }
            Row::Pending(text) => {
                lines.push((
                    index,
                    Line::from(vec![
                        Span::styled(RAIL, Style::new().fg(theme.dim)),
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

/// The prompt of the user: a band across the width with the arrow in front of it.
///
/// This is the look of the grok-build pager: the prompt of the user is the one thing that is
/// banded, so it stands out from the answer of the model.
fn user_lines(text: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let band = Style::new().bg(theme.surface);
    let inner = width.saturating_sub(RAIL.len() + 4).max(20);
    let mut lines = Vec::new();
    for (index, line) in wrap(text, inner).into_iter().enumerate() {
        let mut spans = vec![Span::styled(
            RAIL,
            Style::new().fg(theme.accent_user).bg(theme.surface),
        )];
        if index == 0 {
            spans.push(Span::styled(
                PROMPT_ARROW,
                Style::new().fg(theme.accent_user).bold().bg(theme.surface),
            ));
        } else {
            spans.push(Span::styled("  ", Style::new().bg(theme.surface)));
        }
        spans.push(Span::styled(
            line,
            Style::new().fg(theme.text).bg(theme.surface),
        ));
        // The band goes across the whole width.
        let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        if used < width {
            spans.push(Span::styled(
                " ".repeat(width - used),
                Style::new().bg(theme.surface),
            ));
        }
        lines.push(Line::from(spans).style(band));
    }
    lines
}

/// The answer of the model: plain text with a small, intentional subset of Markdown — bold,
/// italic, inline code, headings, bullets and block quotes. No band, no border.
fn assistant_lines(text: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(RAIL.len()).max(20);
    let mut lines = Vec::new();
    for source in text.lines() {
        let (prefix, content, quoted) = markdown_block(source);
        let styled = markdown_inline(content, theme, quoted);
        let rows = wrap_styled(&styled, inner);
        if rows.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                RAIL,
                Style::new().fg(theme.accent_assistant),
            )]));
            continue;
        }
        for (index, row) in rows.into_iter().enumerate() {
            let mut spans = vec![Span::styled(RAIL, Style::new().fg(theme.accent_assistant))];
            if index == 0 && !prefix.is_empty() {
                spans.push(Span::styled(prefix, Style::new().fg(theme.accent).bold()));
            } else if index > 0 && !prefix.is_empty() {
                spans.push(Span::styled(
                    " ".repeat(prefix.chars().count()),
                    Style::new(),
                ));
            }
            spans.extend(row);
            lines.push(Line::from(spans));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            RAIL,
            Style::new().fg(theme.accent_assistant),
        )]));
    }
    lines
}

/// The block prefix and style of one Markdown source line.
fn markdown_block(source: &str) -> (&'static str, &str, bool) {
    let trimmed = source.trim_start();
    if let Some(rest) = trimmed.strip_prefix("> ") {
        ("│ ", rest, true)
    } else if let Some(rest) = trimmed.strip_prefix('>') {
        ("│ ", rest, true)
    } else if let Some(rest) = trimmed.strip_prefix("### ") {
        ("### ", rest, false)
    } else if let Some(rest) = trimmed.strip_prefix("## ") {
        ("## ", rest, false)
    } else if let Some(rest) = trimmed.strip_prefix("# ") {
        ("# ", rest, false)
    } else if let Some(rest) = trimmed.strip_prefix("- ") {
        ("• ", rest, false)
    } else if let Some(rest) = trimmed.strip_prefix("* ") {
        ("• ", rest, false)
    } else {
        ("", source, false)
    }
}

/// Inline Markdown: `**bold**`, `*italic*`, `_italic_` and `` `code` ``.
fn markdown_inline(text: &str, theme: &Theme, quoted: bool) -> Vec<(char, Style)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0usize;
    let mut bold = false;
    let mut italic = quoted;
    let mut code = false;
    while index < chars.len() {
        if index + 1 < chars.len() && chars[index] == '*' && chars[index + 1] == '*' {
            bold = !bold;
            index += 2;
            continue;
        }
        match chars[index] {
            '`' => {
                code = !code;
                index += 1;
                continue;
            }
            '*' | '_' => {
                italic = !italic;
                index += 1;
                continue;
            }
            ch => {
                let mut style = Style::new().fg(if code {
                    theme.accent
                } else if quoted {
                    theme.text_dim
                } else {
                    theme.text
                });
                if bold {
                    style = style.bold();
                }
                if italic || quoted {
                    style = style.italic();
                }
                if code {
                    style = style.bg(theme.surface);
                }
                out.push((ch, style));
                index += 1;
            }
        }
    }
    out
}

/// Wrap a styled run by display width, keeping each character's style.
fn wrap_styled(styled: &[(char, Style)], width: usize) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<(char, Style)>> = Vec::new();
    let mut row: Vec<(char, Style)> = Vec::new();
    let mut used = 0usize;
    for &(ch, style) in styled {
        let size = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + size > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push((ch, style));
        used += size;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows.into_iter()
        .map(|row| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut text = String::new();
            let mut current: Option<Style> = None;
            for (ch, style) in row {
                if current != Some(style) && !text.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut text),
                        current.unwrap_or_default(),
                    ));
                }
                current = Some(style);
                text.push(ch);
            }
            if !text.is_empty() {
                spans.push(Span::styled(text, current.unwrap_or_default()));
            }
            spans
        })
        .collect()
}

/// Reasoning text: dim, with a thin vertical bar at the left.
fn reasoning_lines(text: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(RAIL.len()).max(20);
    wrap(text, inner)
        .into_iter()
        .map(|line| {
            Line::from(vec![
                Span::styled(RAIL, Style::new().fg(theme.accent_thinking)),
                Span::styled(line, Style::new().fg(theme.dim)),
            ])
        })
        .collect()
}

/// The collapsed reasoning row.
fn reasoning_collapsed(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(RAIL, Style::new().fg(theme.accent_thinking)),
        Span::styled("reasoning", Style::new().fg(theme.dim).italic()),
    ])
}

/// One tool row: `> bash  cargo test  ok 1.2s`.
fn tool_line(
    name: &str,
    intent: Option<&str>,
    command: &str,
    status: &str,
    duration: Option<Duration>,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(RAIL, Style::new().fg(theme.accent_tool)),
        Span::styled(name.to_string(), Style::new().fg(theme.accent_tool).bold()),
    ];
    // The intent of the model: what it says it is doing.
    if let Some(intent) = intent {
        spans.push(Span::styled(
            format!(" {intent}"),
            Style::new().fg(theme.dim).italic(),
        ));
    }
    if !command.is_empty() {
        spans.push(Span::styled(
            format!("  {command}"),
            Style::new().fg(theme.text_dim),
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
    fn a_user_message_is_a_band_with_an_arrow() {
        let theme = Theme::default();
        let lines = transcript_lines(&[Row::User("hi".into())], &theme, true, true, 80);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, 0, "the line knows its row");
        assert!(lines[0].1.to_string().contains(PROMPT_ARROW));
        // The band goes across the whole width.
        assert_eq!(lines[0].1.width(), 80);
    }

    #[test]
    fn every_block_has_a_rail() {
        let theme = Theme::default();
        let rows = [
            Row::User("hi".into()),
            Row::Assistant("hello".into()),
            Row::Reasoning("thinking".into()),
            Row::Tool {
                id: "t1".into(),
                intent: None,
                name: "bash".into(),
                command: "ls".into(),
                status: "ok".into(),
                duration: None,
                detail: None,
                expanded: false,
            },
        ];
        for line in transcript_lines(&rows, &theme, true, true, 80) {
            // The blank line between the blocks has no rail.
            if line.1.spans.is_empty() {
                continue;
            }
            assert!(
                line.1.spans[0].content.contains(RAIL),
                "the block has a rail: {:?}",
                line.1
            );
        }
    }

    #[test]
    fn a_blank_line_sits_between_the_blocks() {
        let theme = Theme::default();
        let rows = [Row::User("hi".into()), Row::Assistant("hello".into())];
        let lines = transcript_lines(&rows, &theme, true, true, 80);
        // The message, the blank line, the answer.
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].1.to_string(), "", "the blocks breathe");
    }

    #[test]
    fn the_answer_of_the_model_is_plain_text() {
        let theme = Theme::default();
        let lines = transcript_lines(&[Row::Assistant("hello".into())], &theme, true, true, 80);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].1.to_string().contains(PROMPT_ARROW));
    }

    #[test]
    fn hidden_reasoning_collapses_to_one_row() {
        let theme = Theme::default();
        let rows = [Row::Reasoning("secret".into())];
        let shown = transcript_lines(&rows, &theme, true, true, 80);
        let hidden = transcript_lines(&rows, &theme, false, true, 80);
        assert_eq!(shown.len(), 1);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].1.to_string().contains("reasoning"));
        assert!(!hidden[0].1.to_string().contains("secret"));
    }

    #[test]
    fn pending_rows_are_tagged() {
        let theme = Theme::default();
        let lines = transcript_lines(&[Row::Pending("later".into())], &theme, true, true, 80);
        assert!(lines[0].1.to_string().contains("queued"));
    }

    #[test]
    fn tool_rows_are_one_line() {
        let theme = Theme::default();
        let rows = [Row::Tool {
            id: "t1".to_string(),
            intent: None,
            name: "bash".into(),
            command: "cargo test".into(),
            status: "ok".into(),
            duration: Some(Duration::from_millis(1200)),
            detail: None,
            expanded: false,
        }];
        let lines = transcript_lines(&rows, &theme, true, true, 80);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].1.to_string().contains("cargo test"));
        assert!(lines[0].1.to_string().contains("1.2s"));
    }

    #[test]
    fn markdown_is_styled_in_the_answer() {
        use ratatui::style::Modifier;
        let theme = Theme::default();
        let text = "**bold** and `code`\n> quoted";
        let lines = transcript_lines(&[Row::Assistant(text.into())], &theme, true, true, 60);
        let spans: Vec<&Span<'static>> = lines
            .iter()
            .flat_map(|(_, line)| line.spans.iter())
            .collect();
        assert!(
            spans.iter().any(|span| span.content.contains("bold")
                && span.style.add_modifier.contains(Modifier::BOLD)),
            "bold is rendered bold: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|span| span.content.contains("code") && span.style.bg == Some(theme.surface)),
            "inline code has a surface: {spans:?}"
        );
        assert!(
            spans.iter().any(|span| span.content.trim() == "│"),
            "the quote has a rail: {spans:?}"
        );
        assert!(
            spans.iter().any(|span| span.content.contains("quoted")
                && span.style.add_modifier.contains(Modifier::ITALIC)),
            "the quote is italic: {spans:?}"
        );
    }

    #[test]
    fn a_long_message_is_wrapped() {
        let theme = Theme::default();
        let text = "word ".repeat(60);
        let lines = transcript_lines(&[Row::Assistant(text)], &theme, true, true, 40);
        // The label, then the wrapped text.
        assert!(
            lines.len() > 3,
            "the message is wrapped: {} lines",
            lines.len()
        );
        for (_, line) in &lines {
            assert!(line.width() <= 40, "the line fits: {line:?}");
        }
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

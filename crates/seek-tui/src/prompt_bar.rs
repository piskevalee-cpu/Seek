//! The prompt bar (SPEC 10.3).
//!
//! A box with rounded corners, the way the grok-build pager draws its prompt: the model name
//! sits in the top border, the text goes in the middle, and the keys are shown in the bottom
//! border. Typing `/` opens one inline suggestion list above the bar, holding slash commands
//! and skills together. Sending while the agent works queues the message.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::{PERMISSION_CHOICES, PromptMode};
use crate::input::{Input, SLASH_COMMANDS};
use crate::theme::Theme;

/// The corners of the box.
pub const TOP_LEFT: &str = "╭";
pub const TOP_RIGHT: &str = "╮";
pub const BOTTOM_LEFT: &str = "╰";
pub const BOTTOM_RIGHT: &str = "╯";
pub const HORIZONTAL: &str = "─";
pub const VERTICAL: &str = "│";

/// Render the input into lines, with a visible cursor.
pub fn prompt_lines(input: &Input, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (row, text) in input_texts(input).iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if row == 0 {
            spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border_active)));
        } else {
            spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border)));
        }
        spans.push(Span::styled(" ", Style::new()));
        let (cursor_row, cursor_column) = input.cursor();
        if row == cursor_row {
            for (column, ch) in text.chars().enumerate() {
                if column == cursor_column {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::new().fg(theme.bg).bg(theme.text),
                    ));
                } else {
                    spans.push(Span::styled(ch.to_string(), Style::new().fg(theme.text)));
                }
            }
            if cursor_column >= text.chars().count() {
                spans.push(Span::styled(" ", Style::new().fg(theme.bg).bg(theme.text)));
            }
        } else {
            spans.push(Span::styled(text.clone(), Style::new().fg(theme.text)));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// The visible text of every input line.
fn input_texts(input: &Input) -> Vec<String> {
    let text = input.text();
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// The suggestion list shown above the bar.
pub fn suggestion_lines(input: &Input, theme: &Theme) -> Vec<Line<'static>> {
    input
        .suggestions()
        .into_iter()
        .map(|command| {
            let description = SLASH_COMMANDS
                .iter()
                .find(|(name, _)| *name == command)
                .map(|(_, description)| *description)
                .unwrap_or("");
            Line::from(vec![
                Span::styled(format!("  {command}"), Style::new().fg(theme.accent_user)),
                Span::styled(format!("  {description}"), Style::new().fg(theme.dim)),
            ])
        })
        .collect()
}

/// The permission selector: three choices, the selected one highlighted.
pub fn permission_lines(mode: &PromptMode, input: &Input, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match mode {
        PromptMode::Normal => {}
        PromptMode::Permission {
            tool,
            summary,
            choice,
            ..
        } => {
            lines.push(Line::from(vec![
                Span::styled("? ", Style::new().fg(theme.warn).bold()),
                Span::styled(
                    format!("{tool} wants to run"),
                    Style::new().fg(theme.warn).bold(),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("  {summary}"),
                Style::new().fg(theme.dim),
            )));
            for (index, label) in PERMISSION_CHOICES.iter().enumerate() {
                let selected = index == *choice;
                let style = if selected {
                    Style::new().fg(theme.bg).bg(theme.warn).bold()
                } else {
                    Style::new().fg(theme.text)
                };
                lines.push(Line::from(Span::styled(
                    format!("{} {label}", if selected { "\u{276F}" } else { " " }),
                    style,
                )));
            }
            lines.push(Line::from(Span::styled(
                "  \u{2191}/\u{2193} choose \u{00b7} Enter confirm \u{00b7} Esc denies",
                Style::new().fg(theme.dim).italic(),
            )));
        }
        PromptMode::Tell { tool, .. } => {
            lines.push(Line::from(vec![
                Span::styled("? ", Style::new().fg(theme.warn).bold()),
                Span::styled(
                    format!("{tool} was denied. What should SEEK do instead?"),
                    Style::new().fg(theme.warn).bold(),
                ),
            ]));
            lines.extend(prompt_lines(input, theme));
        }
    }
    lines
}

/// The top border of the box, with the model name in it.
fn top_border(width: usize, caption: &str, theme: &Theme) -> Line<'static> {
    let style = Style::new().fg(theme.border_active);
    let mut spans = vec![Span::styled(TOP_LEFT, style)];
    // The caption sits in the border, the way grok-build puts the model name there.
    let caption = format!(" {caption} ");
    spans.push(Span::styled(caption, Style::new().fg(theme.dim)));
    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let fill = width.saturating_sub(used + 1);
    spans.push(Span::styled(HORIZONTAL.repeat(fill), style));
    spans.push(Span::styled(TOP_RIGHT, style));
    Line::from(spans)
}

/// The bottom border of the box, with the keys in it.
fn bottom_border(width: usize, hint: &str, theme: &Theme) -> Line<'static> {
    let style = Style::new().fg(theme.border);
    let mut spans = vec![Span::styled(BOTTOM_LEFT, style)];
    let hint = format!(" {hint} ");
    let fill = width.saturating_sub(2 + hint.chars().count()).max(1);
    spans.push(Span::styled(HORIZONTAL.repeat(fill), style));
    spans.push(Span::styled(hint, Style::new().fg(theme.dim)));
    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let fill = width.saturating_sub(used + 1);
    spans.push(Span::styled(HORIZONTAL.repeat(fill), style));
    spans.push(Span::styled(BOTTOM_RIGHT, style));
    Line::from(spans)
}

/// What the prompt bar shows besides the text.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptState {
    /// Whether the agent is working.
    pub busy: bool,
    /// How many messages are queued.
    pub queued: usize,
    /// The mode of the prompt bar: the user types, or the agent asks.
    pub mode: PromptMode,
    /// The caption of the top border: the name of the model.
    pub caption: String,
}

/// Draw the prompt bar.
pub fn render(frame: &mut Frame, area: Rect, input: &Input, theme: &Theme, state: &PromptState) {
    let width = area.width as usize;
    let mut lines = permission_lines(&state.mode, input, theme);

    // The box: the caption in the top border, the keys in the bottom one.
    let mut box_lines = Vec::new();
    box_lines.push(top_border(width, &state.caption, theme));
    if lines.is_empty() {
        lines = prompt_lines(input, theme);
        if state.busy {
            lines.push(Line::from(Span::styled(
                if state.queued > 0 {
                    format!("  {} queued", state.queued)
                } else {
                    "  working...".to_string()
                },
                Style::new().fg(theme.dim).italic(),
            )));
        }
    }
    for line in lines {
        // The right border closes the box.
        let mut spans = line.spans;
        spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border_active)));
        box_lines.push(Line::from(spans));
    }
    let hint = if state.busy {
        "Esc stops"
    } else {
        "Enter sends \u{00b7} Alt+Enter newline"
    };
    box_lines.push(bottom_border(width, hint, theme));

    frame.render_widget(ratatui::widgets::Paragraph::new(box_lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_shows_the_typed_text() {
        let theme = Theme::default();
        let mut input = Input::new();
        for c in "cargo test".chars() {
            input.insert_char(c);
        }
        let lines = prompt_lines(&input, &theme);
        assert!(lines[0].to_string().contains("cargo test"));
    }

    #[test]
    fn suggestions_are_rendered_for_a_slash() {
        let theme = Theme::default();
        let mut input = Input::new();
        for c in "/h".chars() {
            input.insert_char(c);
        }
        let lines = suggestion_lines(&input, &theme);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].to_string().contains("/help"));
    }

    #[test]
    fn a_multi_line_input_renders_one_line_per_row() {
        let theme = Theme::default();
        let mut input = Input::new();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        assert_eq!(prompt_lines(&input, &theme).len(), 2);
    }

    #[test]
    fn the_box_has_corners_and_the_caption() {
        let theme = Theme::default();
        let line = top_border(40, "seek", &theme);
        let text = line.to_string();
        assert!(text.starts_with(TOP_LEFT), "{text}");
        assert!(text.ends_with(TOP_RIGHT), "{text}");
        assert!(text.contains("seek"), "{text}");
        assert_eq!(line.width(), 40);
    }

    #[test]
    fn the_bottom_border_closes_the_box() {
        let theme = Theme::default();
        let line = bottom_border(40, "Enter sends", &theme);
        let text = line.to_string();
        assert!(text.starts_with(BOTTOM_LEFT), "{text}");
        assert!(text.ends_with(BOTTOM_RIGHT), "{text}");
        assert_eq!(line.width(), 40);
    }
}

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

/// The dropdown of the commands, shown above the bar.
///
/// The layout is the one of the grok-build pager (Apache-2.0): a column of the commands, a
/// column of the descriptions, and the picked one highlighted.
pub fn suggestion_lines(
    input: &Input,
    theme: &Theme,
    skills: &[seek_core::Skill],
    selected: usize,
) -> Vec<Line<'static>> {
    let line = input.current_line();
    if !line.starts_with('/') || line.contains(' ') {
        return Vec::new();
    }
    let mut commands: Vec<(String, String)> = SLASH_COMMANDS
        .iter()
        .filter(|(command, _)| command.starts_with(line))
        .map(|(command, description)| ((*command).to_string(), (*description).to_string()))
        .collect();
    for skill in skills {
        let command = format!("/{}", skill.name);
        if command.starts_with(line) && !commands.iter().any(|(known, _)| *known == command) {
            commands.push((command.clone(), skill.description.clone()));
        }
    }
    if commands.is_empty() {
        return Vec::new();
    }

    // The column of the commands is as wide as the longest one.
    let width = commands
        .iter()
        .map(|(command, _)| command.chars().count())
        .max()
        .unwrap_or(0);

    commands
        .iter()
        .enumerate()
        .map(|(index, (command, description))| {
            let picked = index == selected;
            let marker = if picked { "\u{276F} " } else { "  " };
            let mut name = Style::new().fg(if picked {
                theme.accent_user
            } else {
                theme.text
            });
            if picked {
                name = name.bold();
            }
            let mut spans = vec![
                Span::styled(marker.to_string(), name),
                Span::styled(command.clone(), name),
            ];
            // The description is in its own column.
            let pad = width - command.chars().count() + 2;
            spans.push(Span::styled(" ".repeat(pad), Style::new()));
            spans.push(Span::styled(
                description.clone(),
                Style::new().fg(theme.dim),
            ));
            Line::from(spans)
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
        PromptMode::Name => {
            lines.push(Line::from(vec![
                Span::styled("? ", Style::new().fg(theme.accent_user).bold()),
                Span::styled(
                    "What is your name?",
                    Style::new().fg(theme.accent_user).bold(),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "  SEEK will call you by it. Esc leaves it empty.",
                Style::new().fg(theme.dim),
            )));
            lines.extend(prompt_lines(input, theme));
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
fn bottom_border(width: usize, hint: &str, status: &str, theme: &Theme) -> Line<'static> {
    let style = Style::new().fg(theme.border);
    let mut spans = vec![Span::styled(BOTTOM_LEFT, style)];
    let hint = format!(" {hint} ");
    let fill = width.saturating_sub(2 + hint.chars().count()).max(1);
    spans.push(Span::styled(HORIZONTAL.repeat(fill), style));
    spans.push(Span::styled(hint, Style::new().fg(theme.dim)));
    if !status.is_empty() {
        spans.push(Span::styled(
            format!(" {status} "),
            Style::new().fg(theme.dim).italic(),
        ));
    }
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

    // The box: the caption in the top border, the keys and the status in the bottom one.
    // The area of the input holds the text and nothing else, so the cursor is the only
    // thing that moves in it.
    let mut box_lines = Vec::new();
    box_lines.push(top_border(width, &state.caption, theme));
    if lines.is_empty() {
        lines = prompt_lines(input, theme);
    }
    for line in lines {
        // The right border closes the box.
        let mut spans = line.spans;
        spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border_active)));
        box_lines.push(Line::from(spans));
    }
    let status = if state.queued > 0 {
        format!("{} queued", state.queued)
    } else if state.busy {
        "working...".to_string()
    } else {
        String::new()
    };
    let hint = if state.busy {
        "Esc stops"
    } else {
        "Enter sends \u{00b7} Alt+Enter newline"
    };
    box_lines.push(bottom_border(width, hint, &status, theme));

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
        let lines = suggestion_lines(&input, &theme, &[], 0);
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
        let line = bottom_border(40, "Enter sends", "", &theme);
        let text = line.to_string();
        assert!(text.starts_with(BOTTOM_LEFT), "{text}");
        assert!(text.ends_with(BOTTOM_RIGHT), "{text}");
        assert_eq!(line.width(), 40);
    }
}

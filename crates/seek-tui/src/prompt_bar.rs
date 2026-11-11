//! The prompt bar (SPEC 10.3).
//!
//! A multi-line input. Typing `/` opens one inline suggestion list above the bar, holding slash
//! commands and skills together. Sending while the agent works queues the message.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::{PERMISSION_CHOICES, PromptMode};
use crate::input::{Input, SLASH_COMMANDS};
use crate::theme::Theme;

/// Render the input into lines, with a visible cursor.
pub fn prompt_lines(input: &Input, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (row, text) in input_texts(input).iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if row == 0 {
            spans.push(Span::styled("> ", Style::new().fg(theme.accent).bold()));
        } else {
            spans.push(Span::styled("  ", Style::new()));
        }
        let (cursor_row, cursor_column) = input.cursor();
        if row == cursor_row {
            for (column, ch) in text.chars().enumerate() {
                if column == cursor_column {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::new().fg(theme.bg).bg(theme.cube_light),
                    ));
                } else {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::new().fg(theme.cube_light),
                    ));
                }
            }
            if cursor_column >= text.chars().count() {
                spans.push(Span::styled(
                    " ",
                    Style::new().fg(theme.bg).bg(theme.cube_light),
                ));
            }
        } else {
            spans.push(Span::styled(
                text.clone(),
                Style::new().fg(theme.cube_light),
            ));
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
                Span::styled(format!("  {command}"), Style::new().fg(theme.accent)),
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
                let style = if index == *choice {
                    Style::new().fg(theme.bg).bg(theme.accent)
                } else {
                    Style::new().fg(theme.cube_light)
                };
                lines.push(Line::from(Span::styled(format!("  {label}"), style)));
            }
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

/// Draw the prompt bar.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    input: &Input,
    theme: &Theme,
    busy: bool,
    queued: usize,
    mode: &PromptMode,
) {
    let mut lines = permission_lines(mode, input, theme);
    if lines.is_empty() {
        lines = prompt_lines(input, theme);
        if busy {
            lines.push(Line::from(Span::styled(
                if queued > 0 {
                    format!("  {queued} queued")
                } else {
                    "  working...".to_string()
                },
                Style::new().fg(theme.dim).italic(),
            )));
        }
    }
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
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
}

//! The prompt bar (SPEC 10.3).
//!
//! A box with rounded corners, the way the grok-build pager draws its prompt: the working
//! directory sits in the top border, the text goes in the middle, and the keys are shown in the
//! bottom border. The model stays in the footer, so it is not shown twice (SPEC 10.2). Typing `/`
//! opens one inline suggestion list above the bar, holding slash commands and skills together. Sending while the agent works queues the message.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::app::{PERMISSION_CHOICES, PromptMode};
use crate::input::Input;
use crate::theme::Theme;

/// The corners of the box.
pub const TOP_LEFT: &str = "╭";
pub const TOP_RIGHT: &str = "╮";
pub const BOTTOM_LEFT: &str = "╰";
pub const BOTTOM_RIGHT: &str = "╯";
pub const HORIZONTAL: &str = "─";
pub const VERTICAL: &str = "│";

/// Render the input into lines, with a single visible cursor.
///
/// `width` is the usable text width and `max_rows` caps the box; longer drafts scroll inside it.
pub fn prompt_lines(
    input: &Input,
    theme: &Theme,
    width: usize,
    max_rows: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in input.visible_lines(width, max_rows) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border_active)));
        spans.push(Span::styled(" ", Style::new()));
        let cursor = row.cursor.unwrap_or(usize::MAX);
        let mut used = 0usize;
        for ch in row.text.chars() {
            if used == cursor {
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::new().fg(theme.bg).bg(theme.text),
                ));
            } else {
                spans.push(Span::styled(ch.to_string(), Style::new().fg(theme.text)));
            }
            used += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        if cursor >= used {
            spans.push(Span::styled(" ", Style::new().fg(theme.bg).bg(theme.text)));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// The pinned paths of the current draft, shown on one line above the completions.
///
/// This is a view of the `@` tokens in the message, not a second home for them: the message itself
/// carries the pins into the turn (SPEC 10.2).
pub fn pin_lines(pins: &[seek_core::Pin], theme: &Theme) -> Vec<Line<'static>> {
    if pins.is_empty() {
        return Vec::new();
    }
    let text = pins
        .iter()
        .map(|pin| format!("@{}", pin.label))
        .collect::<Vec<_>>()
        .join("   ");
    vec![Line::from(vec![
        Span::styled(" pinned  ", Style::new().fg(theme.bg).bg(theme.accent)),
        Span::styled(text, Style::new().fg(theme.accent)),
    ])]
}

/// The dropdown above the bar: the completions for what is typed.
///
/// The layout is the one of the grok-build pager (Apache-2.0): a column of the names, a column
/// of the descriptions, and the picked one highlighted.
pub fn suggestion_lines(
    items: &[crate::completion::Completion],
    theme: &Theme,
    selected: usize,
) -> Vec<Line<'static>> {
    if items.is_empty() {
        return Vec::new();
    }

    // The column of the names is as wide as the longest one.
    let width = items
        .iter()
        .map(|item| item.label.chars().count())
        .max()
        .unwrap_or(0);

    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
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
                Span::styled(item.label.clone(), name),
            ];
            // The description is in its own column.
            let pad = width - item.label.chars().count() + 2;
            spans.push(Span::styled(" ".repeat(pad), Style::new()));
            spans.push(Span::styled(
                item.detail.clone(),
                Style::new().fg(theme.dim),
            ));
            Line::from(spans)
        })
        .collect()
}

/// The permission selector: three choices, the selected one highlighted.
pub fn permission_lines(
    mode: &PromptMode,
    input: &Input,
    theme: &Theme,
    width: usize,
    max_rows: usize,
) -> Vec<Line<'static>> {
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
            lines.extend(prompt_lines(input, theme, width, max_rows));
        }
        PromptMode::Tell { tool, .. } => {
            lines.push(Line::from(vec![
                Span::styled("? ", Style::new().fg(theme.warn).bold()),
                Span::styled(
                    format!("{tool} was denied. What should SEEK do instead?"),
                    Style::new().fg(theme.warn).bold(),
                ),
            ]));
            lines.extend(prompt_lines(input, theme, width, max_rows));
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
    /// The caption of the top border: the working directory, not the model in the footer.
    pub caption: String,
}

/// Draw the prompt bar.
pub fn render(frame: &mut Frame, area: Rect, input: &Input, theme: &Theme, state: &PromptState) {
    let width = area.width.saturating_sub(3) as usize;
    let max_rows = area.height.saturating_sub(2).max(1) as usize;
    let mut lines = permission_lines(&state.mode, input, theme, width, max_rows);

    // The box: the caption in the top border, the keys and the status in the bottom one.
    // The area of the input holds the text and nothing else, so the cursor is the only
    // thing that moves in it.
    let mut box_lines = Vec::new();
    box_lines.push(top_border(width, &state.caption, theme));
    if lines.is_empty() {
        lines = prompt_lines(input, theme, width, max_rows);
    }
    for line in lines {
        // The right border closes the box.
        let mut spans = line.spans;
        spans.push(Span::styled(VERTICAL, Style::new().fg(theme.border_active)));
        box_lines.push(Line::from(spans));
    }
    // No "working..." status in the border: the thinking animation has its own line above the
    // prompt, so the bottom border only carries the key hint and never gets clipped.
    let hint = if state.busy {
        "Esc stops"
    } else {
        "Enter sends"
    };
    box_lines.push(bottom_border(width, hint, "", theme));

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
        let lines = prompt_lines(&input, &theme, 40, 4);
        assert!(lines[0].to_string().contains("cargo test"));
    }

    #[test]
    fn suggestions_are_rendered_for_a_slash() {
        let theme = Theme::default();
        let mut input = Input::new();
        for c in "/h".chars() {
            input.insert_char(c);
        }
        let items = crate::completion::completions(&input, &[], std::path::Path::new("."));
        let lines = suggestion_lines(&items, &theme, 0);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|line| line.to_string().contains("/help")));
        assert!(
            lines
                .iter()
                .any(|line| line.to_string().contains("/history"))
        );
    }

    #[test]
    fn a_multi_line_input_renders_one_line_per_row() {
        let theme = Theme::default();
        let mut input = Input::new();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        assert_eq!(prompt_lines(&input, &theme, 40, 4).len(), 2);
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

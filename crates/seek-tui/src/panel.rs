//! The shared panel design of the menus (SPEC 10.3).
//!
//! Every overlay — the model and settings menus, the pickers, the help, the launch options and
//! the provider form — uses this vocabulary: a dark-cube border, a dim title with spaces, `accent`
//! section headers, `cube_light` primary text, `dim` secondary text and the directional gradient
//! on the selected row. No background fill: `Clear` has already erased what was behind.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use unicode_width::UnicodeWidthStr;

use crate::gradient;
use crate::theme::Theme;

/// A bordered block that matches the sidebar.
pub fn panel_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(theme.cube_dark))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::new().fg(theme.dim),
        )))
}

/// A section header, like `Sessions` in the sidebar.
pub fn section_header(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::new().fg(theme.accent).bold(),
    ))
}

/// A list row: two-space indent, cursor, label, badge and right-aligned value.
#[allow(clippy::too_many_arguments)]
pub fn row(
    cursor: &str,
    label: &str,
    badge: &str,
    right: &str,
    picked: bool,
    width: usize,
    from_left: bool,
    theme: &Theme,
) -> Line<'static> {
    let indent = "  ";
    let prefix = indent.width() + cursor.width() + 1;
    let fixed = prefix + badge.width() + right.width() + 1;
    let label = truncate(label, width.saturating_sub(fixed));
    let pad = width.saturating_sub(prefix + label.width() + badge.width() + right.width());
    let mut spans = vec![
        Span::styled(indent, Style::new()),
        Span::styled(cursor.to_string(), Style::new().fg(theme.accent)),
        Span::styled(" ", Style::new()),
        Span::styled(label, Style::new().fg(theme.cube_light)),
        Span::styled(badge.to_string(), Style::new().fg(theme.accent)),
        Span::styled(" ".repeat(pad), Style::new()),
        Span::styled(right.to_string(), Style::new().fg(theme.dim)),
    ];
    if picked {
        spans = gradient::paint(spans, width, from_left, theme);
        for span in &mut spans {
            if span.style.add_modifier.is_empty() {
                *span = span.clone().style(span.style.bold());
            }
        }
    }
    Line::from(spans)
}

/// Trim a string to a display width.
pub fn truncate(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + width > max {
            break;
        }
        out.push(ch);
        used += width;
    }
    out
}

/// A dim one-line hint, left aligned under a panel.
pub fn hint(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::new().fg(theme.dim),
    ))
}

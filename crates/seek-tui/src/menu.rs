//! The model selector and the settings menu (SPEC 10.3).
//!
//! Both overlays and the launch-options editor are drawn in the design language of the sidebar:
//! a dark-cube border, a dim title with spaces, `accent` section headers, `cube_light` primary
//! text, `dim` secondary text, and the directional blue gradient on the selected row. Nothing
//! from the grok-build chrome (close button, search row, `◆`, centered shortcuts) remains.

use ratatui::widgets::{BorderType, Clear};
use ratatui::{Frame, layout::Rect, style::Style, text::Line, text::Span};
use unicode_width::UnicodeWidthStr;

use crate::gradient;
use crate::theme::Theme;

/// One row of a menu. `options` makes it a selector; `current` is the active option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub right_label: String,
    pub badge: String,
    pub group: Option<String>,
    pub options: Vec<String>,
    pub current: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: String::new(),
            right_label: String::new(),
            badge: String::new(),
            group: None,
            options: Vec::new(),
            current: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn with_right(mut self, right: impl Into<String>) -> Self {
        self.right_label = right.into();
        self
    }

    pub fn with_badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = badge.into();
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn selecting(mut self, options: &[&str], current: &str) -> Self {
        self.options = options.iter().map(|option| (*option).to_string()).collect();
        self.current = Some(current.to_string());
        self
    }
}

/// What the menu does when a row is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Model,
    Settings,
}

/// A list drawn in the language of the sidebar.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    pub kind: MenuKind,
    pub items: Vec<MenuItem>,
    pub filter: String,
    pub selected: usize,
    pub hint: Option<String>,
}

/// A bordered block that matches the sidebar: dark-cube border, dim title with spaces.
pub(crate) fn panel_block(title: &str, theme: &Theme) -> ratatui::widgets::Block<'static> {
    ratatui::widgets::Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(theme.cube_dark).bg(theme.bg))
        .style(Style::new().bg(theme.bg))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::new().fg(theme.dim),
        )))
}

/// A section header, like `Sessions` in the sidebar.
pub(crate) fn section_header(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::new().fg(theme.accent).bold().bg(theme.bg),
    ))
}

/// The header line: a bold section name and, while filtering, the typed query in dim.
fn header_line(name: &str, filter: &str, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(
        name.to_string(),
        Style::new().fg(theme.accent).bold().bg(theme.bg),
    )];
    if !filter.is_empty() {
        spans.push(Span::styled(
            format!("  {filter}"),
            Style::new().fg(theme.dim).bg(theme.bg),
        ));
    }
    Line::from(spans)
}

/// One list row: two-space indent, a cursor, the label, optional badge and a right-aligned
/// value. The selected row gets the directional gradient.
fn row_line(
    item: &MenuItem,
    right: &str,
    picked: bool,
    width: usize,
    from_left: bool,
    theme: &Theme,
) -> Line<'static> {
    let indent = "  ";
    let cursor = if picked { "\u{276f}" } else { " " };
    let badge = if item.badge.is_empty() {
        String::new()
    } else {
        format!("[{}]", item.badge)
    };
    let prefix_width = indent.width() + cursor.width() + 1;
    let fixed = prefix_width + badge.width() + right.width() + 1;
    let label = truncate(&item.label, width.saturating_sub(fixed));
    let pad = width.saturating_sub(prefix_width + label.width() + badge.width() + right.width());
    let mut spans = vec![
        Span::styled(indent, Style::new().bg(theme.bg)),
        Span::styled(cursor, Style::new().fg(theme.accent).bg(theme.bg)),
        Span::styled(" ", Style::new().bg(theme.bg)),
        Span::styled(label, Style::new().fg(theme.cube_light).bg(theme.bg)),
        Span::styled(badge, Style::new().fg(theme.accent).bg(theme.bg)),
        Span::styled(" ".repeat(pad), Style::new().bg(theme.bg)),
        Span::styled(right.to_string(), Style::new().fg(theme.dim).bg(theme.bg)),
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

impl Menu {
    pub fn new(title: impl Into<String>, kind: MenuKind, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            kind,
            items,
            filter: String::new(),
            selected: 0,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The indices of the items that pass the filter.
    pub fn visible_indices(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                needle.is_empty()
                    || item.label.to_lowercase().contains(&needle)
                    || item.detail.to_lowercase().contains(&needle)
                    || item
                        .group
                        .as_ref()
                        .is_some_and(|group| group.to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        let index = *self.visible_indices().get(self.selected)?;
        self.items.get(index)
    }

    pub fn move_next(&mut self) {
        let count = self.visible_indices().len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    pub fn move_prev(&mut self) {
        let count = self.visible_indices().len();
        if count > 0 {
            self.selected = (self.selected + count - 1) % count;
        }
    }

    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    /// The model selector is an inline list above the prompt, drawn like the sidebar.
    fn render_inline(&self, frame: &mut Frame, area: Rect, theme: &Theme, from_left: bool) {
        let indices = self.visible_indices();
        // A dark divider, the "Models" section header, then the rows.
        let height = (indices.len().max(1) + 2).min(area.height as usize).max(1);
        let rect = Rect::new(
            area.x,
            area.y + area.height - height as u16,
            area.width,
            height as u16,
        );
        frame.render_widget(Clear, rect);
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::new().bg(theme.bg)),
            rect,
        );
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Span::styled(
                "\u{2500}".repeat(rect.width as usize),
                Style::new().fg(theme.cube_dark).bg(theme.bg),
            )),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        frame.render_widget(
            ratatui::widgets::Paragraph::new(header_line("Models", &self.filter, theme)),
            Rect::new(rect.x, rect.y + 1, rect.width, 1),
        );

        let window = height.saturating_sub(2);
        let start = self
            .selected
            .saturating_add(1)
            .saturating_sub(window)
            .min(indices.len().saturating_sub(window));
        for (offset, index) in indices.iter().enumerate().skip(start).take(window) {
            let item = &self.items[*index];
            let right = if item.right_label.is_empty() {
                &item.detail
            } else {
                &item.right_label
            };
            let right = truncate(right, rect.width as usize / 3);
            let y = rect.y + 2 + offset as u16 - start as u16;
            frame.render_widget(
                row_line(
                    item,
                    &right,
                    offset == self.selected,
                    rect.width as usize,
                    from_left,
                    theme,
                ),
                Rect::new(rect.x, y, rect.width, 1),
            );
        }
    }

    /// Draw the settings menu as a sidebar-style panel above the prompt.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme, from_left: bool) {
        if self.kind == MenuKind::Model {
            self.render_inline(frame, area, theme, from_left);
            return;
        }
        frame.render_widget(Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::new().bg(theme.bg)),
            area,
        );

        let max_width = area.width.saturating_sub(4).min(96);
        let width = ((area.width as f32 * 0.8) as u16)
            .min(max_width)
            .max(48)
            .min(area.width);
        let height = area.height.saturating_sub(10).max(9).min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let title = if self.filter.is_empty() {
            self.title.clone()
        } else {
            format!("{}: {}", self.title, self.filter)
        };
        let block = panel_block(&title, theme);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let indices = self.visible_indices();
        // One header row and one hint row are reserved; the rest hold the settings.
        let available = inner.height.saturating_sub(2).max(1) as usize;
        let start = self
            .selected
            .saturating_add(1)
            .saturating_sub(available)
            .min(indices.len().saturating_sub(available));

        frame.render_widget(
            header_line("Settings", "", theme),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        for (offset, index) in indices.iter().enumerate().skip(start).take(available) {
            let item = &self.items[*index];
            let right = truncate(&item.right_label, inner.width as usize / 3);
            let y = inner.y + 1 + offset as u16 - start as u16;
            frame.render_widget(
                row_line(
                    item,
                    &right,
                    offset == self.selected,
                    inner.width as usize,
                    from_left,
                    theme,
                ),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }
        if indices.is_empty() {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Span::styled(
                    "  nothing matches",
                    Style::new().fg(theme.dim).italic().bg(theme.bg),
                )),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
        }
        let hint = self
            .hint
            .as_deref()
            .unwrap_or("\u{2191}\u{2193} move \u{00b7} Enter use \u{00b7} Esc close");
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Span::styled(
                format!("  {hint}"),
                Style::new().fg(theme.dim).bg(theme.bg),
            )),
            Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(1),
                inner.width,
                1,
            ),
        );
    }
}

/// Trim a string to a display width.
fn truncate(text: &str, max: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_filters_and_moves() {
        let mut menu = Menu::new(
            "models",
            MenuKind::Model,
            vec![
                MenuItem::new("Claude", "model:claude"),
                MenuItem::new("GPT", "model:gpt"),
            ],
        );
        menu.type_char('g');
        assert_eq!(menu.visible_indices(), vec![1]);
        assert_eq!(
            menu.selected_item().map(|item| item.label.as_str()),
            Some("GPT")
        );
        menu.backspace();
        menu.move_next();
        assert_eq!(menu.selected, 1);
    }
}

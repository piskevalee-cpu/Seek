//! The model selector and the settings menu (SPEC 10.3).
//!
//! Built from the grok-build pager (Apache-2.0): a wide centered modal with a square border, a
//! bold title on the border, ` search: ` row, full-width divider, picker rows with a right-aligned
//! label, and a centered footer of shortcuts. The picked row uses SEEK's directional blue gradient.

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

/// A grok-style floating menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    pub kind: MenuKind,
    pub items: Vec<MenuItem>,
    pub filter: String,
    pub selected: usize,
    pub hint: Option<String>,
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

    /// The model selector is an inline list above the prompt, like the grok-code picker.
    fn render_inline(&self, frame: &mut Frame, area: Rect, theme: &Theme, from_left: bool) {
        let indices = self.visible_indices();
        let rows = indices.len().max(1);
        let height = (rows + 2).min(area.height as usize).max(1);
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
                Style::new().fg(theme.dim).bg(theme.bg),
            )),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );

        let query = if self.filter.is_empty() {
            "<model> [window] [effort]".to_string()
        } else {
            format!("{} <model> [window] [effort]", self.filter)
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Span::styled(
                format!(" /model {query}"),
                Style::new().fg(theme.dim).bg(theme.bg),
            )),
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
            let picked = offset == self.selected;
            let right = if item.right_label.is_empty() {
                &item.detail
            } else {
                &item.right_label
            };
            let right = truncate(right, rect.width as usize / 3);
            let prefix = if picked { "\u{276f} " } else { "  " };
            let badge = if item.badge.is_empty() {
                String::new()
            } else {
                format!("[{}] ", item.badge)
            };
            let label_width = (rect.width as usize)
                .saturating_sub(prefix.width() + badge.width() + right.width() + 4);
            let label = truncate(&item.label, label_width);
            let pad = (rect.width as usize)
                .saturating_sub(prefix.width() + label.width() + badge.width() + right.width() + 1);
            let mut spans = vec![
                Span::styled(
                    prefix.to_string(),
                    Style::new().fg(theme.accent).bg(theme.bg),
                ),
                Span::styled(label, Style::new().fg(theme.text).bg(theme.bg)),
                Span::styled(badge, Style::new().fg(theme.accent).bg(theme.bg)),
                Span::styled(" ".repeat(pad), Style::new().bg(theme.bg)),
                Span::styled(right, Style::new().fg(theme.dim).bg(theme.bg)),
            ];
            if picked {
                spans = gradient::paint(spans, rect.width as usize, from_left, theme);
                for span in &mut spans {
                    if span.style.add_modifier.is_empty() {
                        *span = span.clone().style(span.style.bold());
                    }
                }
            }
            let y = rect.y + 2 + offset as u16 - start as u16;
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(spans)),
                Rect::new(rect.x, y, rect.width, 1),
            );
        }
    }

    /// Draw the menu on an opaque stage above the prompt.
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

        // grok-build sizing: 90% width, generous vertical margin, wide minimum.
        let max_width = area.width.saturating_sub(4).min(140);
        let width = ((area.width as f32 * 0.9) as u16)
            .min(max_width)
            .max(60)
            .min(area.width);
        let height = area.height.saturating_sub(14).max(8).min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let border_style = Style::new().fg(theme.dim).bg(theme.bg);
        let title_style = Style::new().fg(theme.text).bg(theme.bg).bold();
        let mut block = ratatui::widgets::Block::bordered()
            .border_type(BorderType::Plain)
            .border_style(border_style)
            .style(Style::new().bg(theme.bg))
            .title(Line::from(vec![
                Span::styled("\u{2500} ", border_style),
                Span::styled(self.title.clone(), title_style),
                Span::styled(" \u{2500}", border_style),
            ]));
        // The close button sits on the top-right border, as in grok-build.
        let close = "[✗]";
        let close_x = rect.x + rect.width.saturating_sub(close.len() as u16 + 1);
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Span::styled(
                close,
                Style::new().fg(theme.dim).bg(theme.bg),
            )),
            Rect::new(close_x, rect.y, close.len() as u16, 1),
        );
        if rect.width < 24 {
            // Fall back to a plain title on very narrow terminals.
            block = ratatui::widgets::Block::bordered()
                .border_style(border_style)
                .style(Style::new().bg(theme.bg))
                .title(self.title.clone());
        }
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let content = Rect::new(
            inner.x + 2,
            inner.y + 2,
            inner.width.saturating_sub(4),
            inner.height.saturating_sub(4).max(1),
        );
        let mut canvas: Vec<Line<'static>> = Vec::new();

        // Search row: the label is dim, the query is primary text, the caret is a block.
        let label = " search: ";
        let mut search_spans = vec![Span::styled(label, Style::new().fg(theme.dim).bg(theme.bg))];
        let query_width = (content.width as usize).saturating_sub(label.len() + 1);
        let mut query = truncate(&self.filter, query_width);
        query.push('\u{258c}');
        search_spans.push(Span::styled(
            query,
            Style::new().fg(theme.text).bg(theme.bg),
        ));
        canvas.push(Line::from(search_spans));
        canvas.push(Line::from(Span::styled(
            "\u{2500}".repeat(content.width as usize),
            Style::new().fg(theme.dim).bg(theme.bg),
        )));

        let indices = self.visible_indices();
        let reserved = canvas.len() + 2; // detail line + footer
        let list_height = (content.height as usize).saturating_sub(reserved).max(1);
        let start = self
            .selected
            .saturating_add(1)
            .saturating_sub(list_height)
            .min(indices.len().saturating_sub(list_height));
        let mut selected_at = None;
        let mut last_group: Option<String> = None;
        for (offset, index) in indices.iter().enumerate().skip(start).take(list_height) {
            let item = &self.items[*index];
            if let Some(group) = &item.group
                && last_group.as_deref() != Some(group.as_str())
            {
                last_group = Some(group.clone());
                let label = format!("\u{2500}\u{2500} {group} ");
                let dashes = content.width as usize - label.width().min(content.width as usize);
                canvas.push(Line::from(Span::styled(
                    format!("{label}{}", "\u{2500}".repeat(dashes)),
                    Style::new().fg(theme.dim).bg(theme.bg),
                )));
            }
            if offset == self.selected {
                selected_at = Some(canvas.len());
            }
            let picked = offset == self.selected;
            let prefix = "\u{25c6} "; // ◆ — the grok-build fold indicator
            let badge = if item.badge.is_empty() {
                String::new()
            } else {
                format!("[{}] ", item.badge)
            };
            let right = truncate(&item.right_label, content.width as usize / 3);
            let fixed = prefix.width() + badge.width() + right.width() + 2;
            let label = truncate(&item.label, (content.width as usize).saturating_sub(fixed));
            let pad = content.width as usize
                - (prefix.width() + badge.width() + label.width() + right.width());
            let mut spans = vec![
                Span::styled(prefix.to_string(), Style::new().fg(theme.dim).bg(theme.bg)),
                Span::styled(label, Style::new().fg(theme.text).bg(theme.bg)),
                Span::styled(badge, Style::new().fg(theme.accent).bg(theme.bg)),
                Span::styled(" ".repeat(pad), Style::new().bg(theme.bg)),
                Span::styled(right, Style::new().fg(theme.dim).bg(theme.bg)),
            ];
            if picked {
                spans = gradient::paint(spans, content.width as usize, from_left, theme);
                for span in &mut spans {
                    if span.style.add_modifier.is_empty() {
                        *span = span.clone().style(span.style.bold());
                    }
                }
            }
            canvas.push(Line::from(spans));
        }
        if indices.is_empty() {
            canvas.push(Line::from(Span::styled(
                "  nothing matches",
                Style::new().fg(theme.dim).italic().bg(theme.bg),
            )));
        }

        // The selected row's detail sits below the list, like an expanded picker row.
        if let Some(detail) = self.selected_item().map(|item| item.detail.clone())
            && !detail.is_empty()
        {
            canvas.push(Line::from(Span::styled(
                format!("  {detail}"),
                Style::new().fg(theme.dim).italic().bg(theme.bg),
            )));
        } else {
            canvas.push(Line::from(""));
        }
        let hint = self
            .hint
            .as_deref()
            .unwrap_or("↑↓ move  |  Enter use  |  Esc close");
        canvas.push(Line::from(Span::styled(
            center(hint, content.width as usize),
            Style::new().fg(theme.dim).italic().bg(theme.bg),
        )));

        for (index, line) in canvas.into_iter().enumerate() {
            if index >= content.height as usize {
                break;
            }
            frame.render_widget(
                ratatui::widgets::Paragraph::new(line),
                Rect::new(content.x, content.y + index as u16, content.width, 1),
            );
        }
        let _ = selected_at;
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

/// Centered text of a given width.
fn center(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    if used >= width {
        return text.to_string();
    }
    let left = (width - used) / 2;
    format!("{}{}", " ".repeat(left), text)
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

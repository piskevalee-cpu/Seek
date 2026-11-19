//! The model selector and the settings menu (SPEC 10.3).
//!
//! Built in the style of the grok-build pager (Apache-2.0): a floating panel with a search row, a
//! list on the left and a detail pane on the right. The picked row uses the directional blue
//! gradient from `gradient`.

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
    pub group: Option<String>,
    pub tag: Option<String>,
    pub options: Vec<String>,
    pub current: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: String::new(),
            group: None,
            tag: None,
            options: Vec::new(),
            current: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
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

/// A floating menu with an optional detail pane.
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

    /// The selected item, if any.
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

    /// Draw the menu on an opaque stage above the prompt.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme, from_left: bool) {
        frame.render_widget(Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::new().bg(theme.bg)),
            area,
        );

        let width = area.width.saturating_sub(4).clamp(40, 84).min(area.width);
        let height = area.height.saturating_sub(3).clamp(10, 24).min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + area.height - height,
            width,
            height,
        );
        let block = ratatui::widgets::Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(theme.accent))
            .style(Style::new().bg(theme.bg))
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::new().fg(theme.accent).bold(),
            ));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let mut canvas: Vec<Line<'static>> = Vec::new();
        let search = if self.filter.is_empty() {
            "type to search".to_string()
        } else {
            self.filter.clone()
        };
        let search_style = if self.filter.is_empty() {
            Style::new().fg(theme.dim).italic()
        } else {
            Style::new().fg(theme.cube_light)
        };
        canvas.push(Line::from(vec![
            Span::styled("  ", Style::new()),
            Span::styled("\u{276f} ", Style::new().fg(theme.accent)),
            Span::styled(
                truncate(&search, inner.width.saturating_sub(4) as usize),
                search_style,
            ),
            Span::styled("\u{258c}", Style::new().fg(theme.accent)),
        ]));
        canvas.push(Line::from(Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            Style::new().fg(theme.cube_dark),
        )));

        let indices = self.visible_indices();
        let preview = self.kind == MenuKind::Model && inner.width >= 60;
        let list_width = if preview {
            (inner.width as usize).saturating_mul(3) / 5
        } else {
            inner.width as usize
        };
        let chrome = 4usize; // search, separator, detail, hint
        let list_height = (inner.height as usize).saturating_sub(chrome).max(1);

        let start = self
            .selected
            .saturating_add(1)
            .saturating_sub(list_height)
            .min(indices.len().saturating_sub(list_height));
        let mut selected_line = None;
        for (offset, index) in indices.iter().enumerate().skip(start).take(list_height) {
            let item = &self.items[*index];
            let picked = offset == self.selected;
            if picked {
                selected_line = Some(canvas.len());
            }
            let mut spans = vec![Span::styled(
                if picked { "\u{276f} " } else { "  " }.to_string(),
                Style::new().fg(if picked { theme.accent } else { theme.dim }),
            )];
            if let Some(tag) = &item.tag {
                spans.push(Span::styled(
                    format!("{tag} "),
                    Style::new().fg(theme.accent),
                ));
            }
            spans.push(Span::styled(
                truncate(&item.label, list_width.saturating_sub(6)),
                Style::new().fg(theme.text),
            ));
            if picked {
                spans = gradient::paint(spans, list_width, from_left, theme);
            }
            canvas.push(Line::from(spans));
        }
        if indices.is_empty() {
            canvas.push(Line::from(Span::styled(
                "  nothing matches",
                Style::new().fg(theme.dim).italic(),
            )));
        }

        let detail = self
            .selected_item()
            .map(|item| item.detail.clone())
            .unwrap_or_default();
        canvas.push(Line::from(Span::styled(
            format!(
                "  {}",
                truncate(&detail, inner.width.saturating_sub(2) as usize)
            ),
            Style::new().fg(theme.dim),
        )));
        let hint = self
            .hint
            .as_deref()
            .unwrap_or("↑↓ choose · Enter select · Esc close");
        canvas.push(Line::from(Span::styled(
            format!("  {hint}"),
            Style::new().fg(theme.dim).italic(),
        )));

        // The detail pane takes the right side of the list area.
        let pane_x = inner.x + list_width as u16;
        if preview {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                    "\u{2502}",
                    Style::new().fg(theme.cube_dark),
                ))),
                Rect::new(pane_x, inner.y + 2, 1, list_height as u16),
            );
        }
        let _ = selected_line;

        for (index, line) in canvas.into_iter().enumerate() {
            if index >= inner.height as usize {
                break;
            }
            frame.render_widget(
                ratatui::widgets::Paragraph::new(line),
                Rect::new(inner.x, inner.y + index as u16, list_width as u16, 1),
            );
        }

        if preview && let Some(item) = self.selected_item() {
            let pane_width = (inner.width as usize).saturating_sub(list_width + 1);
            let lines = wrap(&item.detail, pane_width.saturating_sub(2));
            for (index, line) in lines.into_iter().enumerate().take(list_height) {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(Span::styled(
                        format!(" {line}"),
                        Style::new().fg(theme.text_dim),
                    )),
                    Rect::new(pane_x + 1, inner.y + 2 + index as u16, pane_width as u16, 1),
                );
            }
        }
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

/// Wrap plain text into lines of at most `width` display columns.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let width = width.max(1);
        match lines.last_mut() {
            Some(line)
                if UnicodeWidthStr::width(line.as_str()) + 1 + UnicodeWidthStr::width(word)
                    <= width =>
            {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
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

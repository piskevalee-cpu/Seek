//! The picker overlay (SPEC 10.3).
//!
//! One reusable centered overlay with a filterable list. `/model` groups by provider and flags
//! local models, `/provider` shows the configured providers, `/tools` and `/skills` are
//! checklists.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// One entry of a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// The main text.
    pub label: String,
    /// A second line with details.
    pub detail: String,
    /// The group the item belongs to, e.g. the provider name.
    pub group: Option<String>,
    /// A short tag, e.g. `LOCAL`.
    pub tag: Option<String>,
    /// The value the picker returns when the item is chosen.
    pub value: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        PickerItem {
            label: label.into(),
            detail: String::new(),
            group: None,
            tag: None,
            value: value.into(),
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
}

/// A filterable list.
#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    pub title: String,
    pub filter: String,
    pub items: Vec<PickerItem>,
    /// The index of the selected item, within the visible ones.
    pub selected: usize,
    /// A hint shown under the list.
    pub hint: Option<String>,
}

impl Picker {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Picker {
            title: title.into(),
            filter: String::new(),
            items,
            selected: 0,
            hint: None,
        }
    }

    /// The items that pass the filter.
    pub fn visible(&self) -> Vec<&PickerItem> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                if filter.is_empty() {
                    return true;
                }
                item.label.to_lowercase().contains(&filter)
                    || item.detail.to_lowercase().contains(&filter)
                    || item
                        .group
                        .as_ref()
                        .is_some_and(|group| group.to_lowercase().contains(&filter))
            })
            .collect()
    }

    /// The selected item.
    pub fn selected_item(&self) -> Option<&PickerItem> {
        self.visible().get(self.selected).copied()
    }

    /// Set the hint shown under the list.
    pub fn tap_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Move the selection down.
    pub fn move_next(&mut self) {
        let count = self.visible().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % count;
    }

    /// Move the selection up.
    pub fn move_prev(&mut self) {
        let count = self.visible().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + count - 1) % count;
    }

    /// Add a character to the filter.
    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    /// Remove the last character of the filter.
    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    /// Draw the picker.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let width = area.width.min(70);
        let height = area.height.min(20);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        // The overlay is opaque: nothing behind it shows through.
        frame.render_widget(ratatui::widgets::Clear, rect);
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::new().fg(theme.accent))
            .style(Style::new().bg(theme.bg))
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::new().fg(theme.accent).bold(),
            ));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("> ", Style::new().fg(theme.accent)),
            Span::styled(self.filter.clone(), Style::new().fg(theme.cube_light)),
        ]));

        let items = self.visible();
        if items.is_empty() {
            lines.push(Line::from(Span::styled(
                "  nothing matches",
                Style::new().fg(theme.dim).italic(),
            )));
        }
        let mut last_group: Option<&str> = None;
        for (index, item) in items.iter().enumerate() {
            if let Some(group) = &item.group
                && last_group != Some(group.as_str())
            {
                lines.push(Line::from(Span::styled(
                    format!("  {group}"),
                    Style::new().fg(theme.dim).bold(),
                )));
                last_group = Some(group.as_str());
            }
            let selected = index == self.selected;
            let style = if selected {
                Style::new().fg(theme.bg).bg(theme.accent)
            } else {
                Style::new().fg(theme.cube_light)
            };
            let mut spans = vec![Span::styled(format!("  {}", item.label), style)];
            if let Some(tag) = &item.tag {
                spans.push(Span::styled(
                    format!("  {tag}"),
                    Style::new().fg(if selected { theme.bg } else { theme.accent }),
                ));
            }
            lines.push(Line::from(spans));
            if selected && !item.detail.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("    {}", item.detail),
                    Style::new().fg(theme.dim),
                )));
            }
        }

        if let Some(hint) = &self.hint {
            lines.push(Line::from(Span::styled(
                format!("  {hint}"),
                Style::new().fg(theme.dim).italic(),
            )));
        }

        frame.render_widget(
            ratatui::widgets::Paragraph::new(lines),
            Rect::new(inner.x, inner.y, inner.width, inner.height),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker() -> Picker {
        Picker::new(
            "models",
            vec![
                PickerItem::new("Claude Sonnet 4.5", "anthropic/claude-sonnet-4-5")
                    .with_group("anthropic")
                    .with_detail("200k context"),
                PickerItem::new("GPT-5.4", "openai/gpt-5.4").with_group("openai"),
                PickerItem::new("Local model", "local/mistral").with_tag("LOCAL"),
            ],
        )
    }

    #[test]
    fn the_filter_narrows_the_list() {
        let mut picker = picker();
        assert_eq!(picker.visible().len(), 3);
        for c in "sonnet".chars() {
            picker.type_char(c);
        }
        assert_eq!(picker.visible().len(), 1);
        for _ in 0.."sonnet".len() {
            picker.backspace();
        }
        assert_eq!(picker.visible().len(), 3);
    }

    #[test]
    fn the_filter_matches_the_group() {
        let mut picker = picker();
        for c in "anthropic".chars() {
            picker.type_char(c);
        }
        assert_eq!(picker.visible().len(), 1);
    }

    #[test]
    fn the_selection_wraps_around() {
        let mut picker = picker();
        assert_eq!(picker.selected, 0);
        picker.move_next();
        picker.move_next();
        picker.move_next();
        assert_eq!(picker.selected, 0);
        picker.move_prev();
        assert_eq!(picker.selected, 2);
    }

    #[test]
    fn the_selected_item_is_returned() {
        let mut picker = picker();
        assert_eq!(
            picker.selected_item().map(|item| item.value.as_str()),
            Some("anthropic/claude-sonnet-4-5")
        );
        picker.move_next();
        assert_eq!(
            picker.selected_item().map(|item| item.value.as_str()),
            Some("openai/gpt-5.4")
        );
    }

    #[test]
    fn filtering_moves_the_selection_back_to_the_top() {
        let mut picker = picker();
        picker.move_next();
        picker.type_char('g');
        assert_eq!(picker.selected, 0);
    }
}

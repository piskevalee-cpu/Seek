//! The picker overlay (SPEC 10.3).
//!
//! One reusable floating panel with a search row, grouped rows, a side scrollbar and a detail
//! line for the selection. It follows the compact pickers of OMP (MIT) and the grok-build pager
//! (Apache-2.0): a list you can scan, not a modal wall of text.

use ratatui::widgets::Clear;
use ratatui::{Frame, layout::Rect, style::Style, text::Line, text::Span};

use crate::scrollbar;
use crate::theme::Theme;

/// One entry of a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// The main text.
    pub label: String,
    /// Whether the item is checked. A checked list is a checklist.
    pub checked: Option<bool>,
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
            checked: None,
            detail: String::new(),
            group: None,
            tag: None,
            value: value.into(),
        }
    }

    /// Make the item a checkbox.
    pub fn with_check(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Turn the check on or off.
    pub fn toggle(&mut self) {
        if let Some(checked) = self.checked.as_mut() {
            *checked = !*checked;
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
    /// A model-style picker is anchored above the prompt bar instead of centered.
    pub bottom: bool,
}

impl Picker {
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Picker {
            title: title.into(),
            filter: String::new(),
            items,
            selected: 0,
            hint: None,
            bottom: false,
        }
    }

    /// Anchor the picker above the prompt bar, like the `@` completion list.
    pub fn bottom_anchored(mut self) -> Self {
        self.bottom = true;
        self
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

    /// The indices of the items that pass the filter.
    pub fn visible_indices(&self) -> Vec<usize> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                filter.is_empty()
                    || item.label.to_lowercase().contains(&filter)
                    || item.detail.to_lowercase().contains(&filter)
                    || item
                        .group
                        .as_ref()
                        .is_some_and(|group| group.to_lowercase().contains(&filter))
            })
            .map(|(index, _)| index)
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
    /// Draw the picker as a sidebar-style panel on an opaque stage.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme, from_left: bool) {
        frame.render_widget(Clear, area);

        let count = self.visible().len();
        let width = area.width.saturating_sub(4).clamp(24, 76).min(area.width);
        // Fit the content when the list is short, and stop growing at a comfortable maximum.
        let wanted = 2 + count.max(1) + 2 + 2;
        let height = wanted.clamp(9, 22);
        let height = height
            .min(area.height.saturating_sub(3) as usize)
            .min(area.height as usize) as u16;
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + area.height - height,
            width,
            height,
        );
        let title = if self.filter.is_empty() {
            if count == 0 {
                self.title.clone()
            } else {
                format!("{} · {}", self.title, count)
            }
        } else {
            format!("{}: {}", self.title, self.filter)
        };
        let block = crate::panel::panel_block(&title, theme);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let text_width = inner.width.saturating_sub(2) as usize;

        // Grouped rows, like the sections of the sidebar.
        let items = self.visible();
        let mut rows: Vec<Line<'static>> = Vec::new();
        let mut selected_row = 0usize;
        let mut last_group: Option<String> = None;
        for (index, item) in items.iter().enumerate() {
            if let Some(group) = &item.group
                && last_group.as_deref() != Some(group.as_str())
            {
                last_group = Some(group.clone());
                rows.push(crate::panel::section_header(group, theme));
            }
            if index == self.selected {
                selected_row = rows.len();
            }
            let picked = index == self.selected;
            let cursor = if picked { "\u{276f}" } else { " " };
            let badge = match item.checked {
                Some(true) => "[x]",
                Some(false) => "[ ]",
                None => "",
            };
            let right = item.tag.as_deref().unwrap_or("");
            let right = crate::panel::truncate(right, text_width / 3);
            let mut line = crate::panel::row(
                cursor,
                &item.label,
                badge,
                &right,
                picked,
                inner.width as usize,
                from_left,
                theme,
            );
            // A one-column thumb keeps the picker scrollable without a separate bar.
            let overflow = items.len() > rows.len().saturating_add(6);
            if overflow {
                let color = if index == self.selected {
                    theme.accent
                } else {
                    theme.cube_dark
                };
                let mut spans = vec![
                    Span::styled(scrollbar::THUMB, Style::new().fg(color)),
                    Span::styled(" ", Style::new()),
                ];
                spans.extend(line.spans);
                line = Line::from(spans);
            }
            rows.push(line);
        }
        if rows.is_empty() {
            rows.push(Line::from(Span::styled(
                "  none",
                Style::new().fg(theme.dim).italic(),
            )));
        }

        // One detail line and one hint are fixed; the rows between them scroll.
        let detail = items
            .get(self.selected)
            .map(|item| item.detail.clone())
            .unwrap_or_default();
        let available = inner.height.saturating_sub(2).max(1) as usize;
        let start = selected_row
            .saturating_add(1)
            .saturating_sub(available)
            .min(rows.len().saturating_sub(available));
        for (offset, row) in rows.iter().skip(start).take(available).enumerate() {
            frame.render_widget(
                row.clone(),
                Rect::new(inner.x, inner.y + offset as u16, inner.width, 1),
            );
        }
        frame.render_widget(
            Line::from(Span::styled(
                crate::panel::truncate(&detail, text_width),
                Style::new().fg(theme.dim),
            )),
            Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(2),
                inner.width,
                1,
            ),
        );
        let hint = self
            .hint
            .as_deref()
            .unwrap_or("↑↓ choose · Enter use · Esc close");
        frame.render_widget(
            crate::panel::hint(hint, theme),
            Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(1),
                inner.width,
                1,
            ),
        );
    }
}

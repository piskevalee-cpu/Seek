//! The picker overlay (SPEC 10.3).
//!
//! One reusable floating panel with a search row, grouped rows, a side scrollbar and a detail
//! line for the selection. It follows the compact pickers of OMP (MIT) and the grok-build pager
//! (Apache-2.0): a list you can scan, not a modal wall of text.

use ratatui::widgets::{BorderType, Clear};
use ratatui::{Frame, layout::Rect, style::Style, text::Line, text::Span};
use unicode_width::UnicodeWidthStr;

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
    /// Draw the picker as a floating panel on an opaque stage.
    ///
    /// The panel has a search row, grouped rows, a side scrollbar and a detail line for the
    /// selection. It is anchored above the prompt, so the box stays visible while choosing.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::new().bg(theme.bg)),
            area,
        );

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
        let title = if count == 0 {
            self.title.clone()
        } else {
            format!("{} · {}", self.title, count)
        };
        let block = ratatui::widgets::Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(theme.accent))
            .style(Style::new().bg(theme.bg))
            .title(Span::styled(
                format!(" {title} "),
                Style::new().fg(theme.accent).bold(),
            ));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let mut canvas: Vec<Line<'static>> = Vec::with_capacity(inner.height as usize);
        let text_width = inner.width.saturating_sub(2) as usize;

        // Search row, with a caret so it reads like the prompt rather than a label.
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
                truncate(&search, text_width.saturating_sub(2)),
                search_style,
            ),
            Span::styled("\u{258c}", Style::new().fg(theme.accent)),
        ]));
        canvas.push(Line::from(Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            Style::new().fg(theme.cube_dark),
        )));

        // Build all rows first, so the window can be clamped around the selection.
        let items = self.visible();
        let mut rows: Vec<Line<'static>> = Vec::new();
        let mut selected_row = 0usize;
        let mut last_group: Option<String> = None;
        for (index, item) in items.iter().enumerate() {
            if let Some(group) = &item.group
                && last_group.as_deref() != Some(group.as_str())
            {
                let label = format!("  {group} ");
                let dashes = text_width.saturating_sub(UnicodeWidthStr::width(label.as_str()));
                rows.push(Line::from(vec![
                    Span::styled(label, Style::new().fg(theme.dim).bold()),
                    Span::styled("\u{2500}".repeat(dashes), Style::new().fg(theme.cube_dark)),
                ]));
                last_group = Some(group.clone());
            }
            if index == self.selected {
                selected_row = rows.len();
            }
            let picked = index == self.selected;
            let mut spans = vec![Span::styled(
                if picked { "\u{276f} " } else { "  " }.to_string(),
                Style::new().fg(if picked { theme.accent } else { theme.dim }),
            )];
            if let Some(checked) = item.checked {
                spans.push(Span::styled(
                    if checked { "[x] " } else { "[ ] " }.to_string(),
                    Style::new().fg(if picked { theme.accent } else { theme.text_dim }),
                ));
            }
            let name = truncate(&item.label, text_width / 2);
            spans.push(Span::styled(
                name,
                Style::new().fg(if picked { theme.accent } else { theme.text }),
            ));
            if let Some(tag) = &item.tag {
                spans.push(Span::styled(
                    format!("  {tag}"),
                    Style::new().fg(if picked { theme.bg } else { theme.accent }),
                ));
            }
            rows.push(Line::from(spans));
        }
        if rows.is_empty() {
            rows.push(Line::from(Span::styled(
                "  nothing matches",
                Style::new().fg(theme.dim).italic(),
            )));
        }

        // The detail line and the key hint are fixed; everything between scrolls.
        let detail = items
            .get(self.selected)
            .map(|item| item.detail.clone())
            .unwrap_or_default();
        let chrome: usize = 2 + 1 + 1; // search + separator + detail + hint
        let list_height = (inner.height as usize).saturating_sub(chrome).max(1);
        let overflow = rows.len() > list_height;
        let start = selected_row
            .saturating_add(1)
            .saturating_sub(list_height)
            .min(rows.len().saturating_sub(list_height));
        let thumb = if overflow {
            Some(scrollbar::thumb_range(list_height, rows.len(), start))
        } else {
            None
        };

        for (offset, (_, row)) in rows
            .iter()
            .enumerate()
            .skip(start)
            .take(list_height)
            .enumerate()
        {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let right = if let Some(range) = &thumb {
                let row_index = start + offset;
                let color = if range.contains(&row_index) {
                    theme.accent
                } else {
                    theme.cube_dark
                };
                spans.push(Span::styled(
                    scrollbar::THUMB.to_string(),
                    Style::new().fg(color),
                ));
                "  "
            } else {
                ""
            };
            spans.extend(row.spans.iter().cloned());
            if !right.is_empty() {
                spans.push(Span::styled(right.to_string(), Style::new()));
            }
            canvas.push(Line::from(spans));
        }

        canvas.push(Line::from(Span::styled(
            format!("  {}", truncate(&detail, text_width.saturating_sub(2))),
            Style::new().fg(theme.dim),
        )));
        let hint = self
            .hint
            .as_deref()
            .unwrap_or("↑↓ choose · Enter use · Esc close");
        canvas.push(Line::from(Span::styled(
            format!("  {hint}"),
            Style::new().fg(theme.dim).italic(),
        )));

        for (index, line) in canvas.into_iter().enumerate() {
            if index >= inner.height as usize {
                break;
            }
            frame.render_widget(
                ratatui::widgets::Paragraph::new(line),
                Rect::new(inner.x + 1, inner.y + index as u16, inner.width, 1),
            );
        }
    }
}

/// Trim a string to a display-width, without cutting a wide character.
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

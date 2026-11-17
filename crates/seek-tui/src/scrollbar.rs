//! The side scrollbar of the transcript (SPEC 10.3).
//!
//! The geometry follows the oh-my-pi scroll view (MIT): one offset for the first visible row,
//! clamped to `[0, total - height]`, and a thumb whose size is the viewport over the content.
//! The bar itself is one column; it is drawn on the side opposite the sidebar.
//!
//! This module only knows rows and columns. `App` owns the offset and decides where to draw.

use std::ops::Range;

use ratatui::{Frame, layout::Rect, style::Style, text::Line, text::Span};

use crate::theme::Theme;

/// The character of the track.
pub const TRACK: &str = "│";
/// The character of the thumb.
pub const THUMB: &str = "█";

/// The last legal offset for a viewport of `height` rows over `total` rows.
pub fn max_offset(total: usize, height: usize) -> usize {
    total.saturating_sub(height)
}

/// Keep an offset inside the viewport.
pub fn clamp_offset(offset: usize, total: usize, height: usize) -> usize {
    offset.min(max_offset(total, height))
}

/// The half-open range of visible rows.
pub fn visible_range(total: usize, height: usize, offset: usize) -> Range<usize> {
    let start = clamp_offset(offset, total, height);
    start..total.min(start.saturating_add(height))
}

/// The half-open range of viewport rows the thumb covers.
pub fn thumb_range(height: usize, total: usize, offset: usize) -> Range<usize> {
    if height == 0 {
        return 0..0;
    }
    if total <= height {
        return 0..height;
    }
    // A third of the viewport for a document three times as long, and never less than a cell.
    let size = (height * height / total).clamp(1, height);
    let travel = height - size;
    let max = max_offset(total, height);
    let start = (offset.saturating_mul(travel) + max / 2)
        .checked_div(max)
        .unwrap_or(0);
    start..start.saturating_add(size)
}

/// The offset a drag to a viewport row means.
///
/// Clicking above the thumb pages up, clicking below it pages down; dragging the thumb maps its
/// top row linearly to the offset range.
pub fn offset_at_row(height: usize, total: usize, row: usize) -> usize {
    let max = max_offset(total, height);
    let thumb = thumb_range(height, total, 0);
    let travel = height.saturating_sub(thumb.end - thumb.start);
    if travel == 0 {
        return 0;
    }
    let wanted = row.saturating_sub(thumb.start);
    (wanted.saturating_mul(max) + travel / 2) / travel
}

/// Draw the bar. Nothing is drawn when the content fits the viewport.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, total: usize, offset: usize) {
    let height = area.height as usize;
    if height == 0 || area.width == 0 || total <= height {
        return;
    }
    let thumb = thumb_range(height, total, offset);
    let lines: Vec<Line<'static>> = (0..height)
        .map(|row| {
            let (text, color) = if (thumb.start..thumb.end).contains(&row) {
                (THUMB, theme.accent)
            } else {
                (TRACK, theme.cube_dark)
            };
            Line::from(Span::styled(text.to_string(), Style::new().fg(color)))
        })
        .collect();
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_is_clamped_to_the_content() {
        assert_eq!(max_offset(100, 10), 90);
        assert_eq!(max_offset(5, 10), 0);
        assert_eq!(clamp_offset(200, 100, 10), 90);
        assert_eq!(clamp_offset(20, 100, 10), 20);
    }

    #[test]
    fn the_visible_range_is_half_open() {
        assert_eq!(visible_range(100, 10, 0), 0..10);
        assert_eq!(visible_range(100, 10, 90), 90..100);
        assert_eq!(visible_range(100, 10, 200), 90..100);
    }

    #[test]
    fn the_thumb_is_a_fraction_of_the_viewport() {
        // Ten visible rows out of a hundred: a one-row thumb with nine rows of travel.
        assert_eq!(thumb_range(10, 100, 0), 0..1);
        assert_eq!(thumb_range(10, 100, 45), 5..6);
        assert_eq!(thumb_range(10, 100, 90), 9..10);
    }

    #[test]
    fn a_short_content_has_no_travel() {
        assert_eq!(thumb_range(10, 5, 0), 0..10);
        assert_eq!(thumb_range(0, 100, 0), 0..0);
    }

    #[test]
    fn a_drag_maps_the_thumb_to_the_offset() {
        assert_eq!(offset_at_row(10, 100, 0), 0);
        assert_eq!(offset_at_row(10, 100, 5), 50);
        assert_eq!(offset_at_row(10, 100, 9), 90);
    }
}

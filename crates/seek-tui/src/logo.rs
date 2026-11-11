//! Drawing the logo (SPEC 12).
//!
//! The bitmap is a constant extracted from `assets/logo.png` by `tools/extract_logo.py`; the PNG
//! is never scaled or read at runtime.
//!
//! * Full blocks: every pixel is two columns wide, so the logo is 50 x 9 cells.
//! * Half blocks: on short terminals every pixel is one column and two pixels share a row,
//!   so the logo is 25 x 5 cells.
//! * Below 56 columns the word SEEK is shown as plain text.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::logo_bitmap::{LOGO, LOGO_COLS, LOGO_ROWS};
use crate::theme::Theme;

/// Below this width the logo is replaced by the word SEEK.
pub const PLAIN_TEXT_MIN_WIDTH: u16 = 56;

/// The height needed for the full-block rendering.
pub const FULL_BLOCK_HEIGHT: u16 = LOGO_ROWS as u16;

/// A pixel of the logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Dark,
    Dim,
    Lit,
}

fn level(cell: char) -> Level {
    match cell {
        '#' => Level::Lit,
        '.' => Level::Dim,
        _ => Level::Dark,
    }
}

fn style_for(level: Level, theme: &Theme) -> Style {
    match level {
        Level::Lit => Style::new().fg(theme.cube_light),
        Level::Dim => Style::new().fg(theme.dim),
        Level::Dark => Style::new().fg(theme.cube_dark),
    }
}

/// The logo as text lines, without positioning.
pub fn logo_lines(theme: &Theme, half_blocks: bool) -> Vec<Line<'static>> {
    if half_blocks {
        half_block_lines(theme)
    } else {
        full_block_lines(theme)
    }
}

/// Every pixel drawn as two columns of full blocks: 50 x 9 cells.
fn full_block_lines(theme: &Theme) -> Vec<Line<'static>> {
    LOGO.iter()
        .map(|row| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for cell in row.chars() {
                let level = level(cell);
                spans.push(Span::styled("██", style_for(level, theme)));
            }
            Line::from(spans)
        })
        .collect()
}

/// Every pixel drawn as one column, two pixels per row: 25 x 5 cells.
fn half_block_lines(theme: &Theme) -> Vec<Line<'static>> {
    const UPPER: &str = "▀";
    const LOWER: &str = "▄";
    const FULL: &str = "█";

    let mut lines = Vec::new();
    let mut top = 0;
    while top < LOGO_ROWS {
        let bottom = top + 1;
        let top_row = LOGO[top].chars().collect::<Vec<_>>();
        let bottom_row = LOGO.get(bottom).map(|r| r.chars().collect::<Vec<_>>());

        let mut spans: Vec<Span<'static>> = Vec::new();
        for column in 0..LOGO_COLS {
            let top_level = level(top_row[column]);
            let bottom_level = bottom_row
                .as_ref()
                .map(|row| level(row[column]))
                .unwrap_or(Level::Dark);
            let (glyph, style) = match (top_level, bottom_level) {
                (Level::Lit, Level::Lit) => (FULL, style_for(Level::Lit, theme)),
                (Level::Lit, _) => (UPPER, style_for(Level::Lit, theme)),
                (_, Level::Lit) => (LOWER, style_for(Level::Lit, theme)),
                (Level::Dim, Level::Dim) => (FULL, style_for(Level::Dim, theme)),
                (Level::Dim, _) => (UPPER, style_for(Level::Dim, theme)),
                (_, Level::Dim) => (LOWER, style_for(Level::Dim, theme)),
                (Level::Dark, Level::Dark) => (" ", style_for(Level::Dark, theme)),
            };
            spans.push(Span::styled(glyph, style));
        }
        lines.push(Line::from(spans));
        top += 2;
    }
    lines
}

/// Draw the logo, centered in `area`.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    if area.width < PLAIN_TEXT_MIN_WIDTH {
        let text = Line::from(Span::styled(
            "SEEK",
            Style::new().fg(theme.cube_light).bold(),
        ));
        let width = area.width.min(text.width() as u16);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        frame.render_widget(
            ratatui::widgets::Paragraph::new(text),
            Rect::new(x, area.y, width, 1),
        );
        return;
    }

    let half_blocks = area.height < FULL_BLOCK_HEIGHT;
    let lines = logo_lines(theme, half_blocks);
    let width = lines
        .first()
        .map(|line| line.width() as u16)
        .unwrap_or(0)
        .min(area.width);
    let height = lines.len() as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    // The logo sits in the middle of the area, the widgets go below it.
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    for (index, line) in lines.iter().enumerate() {
        let row = y + index as u16;
        if row >= area.y + area.height {
            break;
        }
        frame.render_widget(
            ratatui::widgets::Paragraph::new(line.clone()),
            Rect::new(x, row, width, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_block_rendering_is_two_columns_per_pixel() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, false);
        assert_eq!(lines.len(), LOGO_ROWS);
        assert_eq!(lines[0].width(), LOGO_COLS * 2);
    }

    #[test]
    fn half_block_rendering_halves_the_height() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, true);
        assert_eq!(lines.len(), LOGO_ROWS.div_ceil(2));
        assert_eq!(lines[0].width(), LOGO_COLS);
    }

    #[test]
    fn the_bitmap_has_the_expected_shape() {
        assert_eq!(LOGO_COLS, 25);
        assert_eq!(LOGO_ROWS, 9);
        for row in LOGO {
            assert_eq!(row.chars().count(), LOGO_COLS);
        }
    }
}

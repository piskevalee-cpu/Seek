//! Drawing the logo (SPEC 12).
//!
//! The bitmap is a constant extracted from `assets/logo.png` by `tools/extract_logo.py`; the PNG is
//! never scaled or read at runtime.
//!
//! * Full blocks: every pixel is two columns wide, so the logo is 50 x 9 cells.
//! * Half blocks: on short terminals every pixel is one column and two pixels share a row,
//!   so the logo is 25 x 5 cells.
//! * Below 56 columns the word SEEK is shown as plain text.
//!
//! Only the lit pixels are drawn: the background of the logo is the background of the terminal,
//! not a block of colour. A sheen sweeps across the mark, the way the grok-build pager does it
//! (`docs/DECISIONS.md` D-010).

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::logo_bitmap::{LOGO, LOGO_COLS, LOGO_ROWS};
use crate::shimmer::{blend, shine_opacity};
use crate::theme::Theme;

/// Below this width the logo is replaced by the word SEEK.
pub const PLAIN_TEXT_MIN_WIDTH: u16 = 56;

/// The height needed for the full-block rendering.
pub const FULL_BLOCK_HEIGHT: u16 = LOGO_ROWS as u16;

/// A pixel of the logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// Nothing is drawn.
    Dark,
    /// A faint pixel: a sparkle.
    Dim,
    /// A lit pixel.
    Lit,
}

fn level(cell: char) -> Level {
    match cell {
        '#' => Level::Lit,
        '.' => Level::Dim,
        _ => Level::Dark,
    }
}

/// The colour of a pixel at a point in the sweep.
///
/// The resting colour is the lit colour pulled towards the background; the sweep lifts it up to
/// the lit colour again.
fn pixel_color(cell: Level, theme: &Theme, opacity: f32) -> Option<Color> {
    match cell {
        Level::Dark => None,
        Level::Dim => {
            let resting = blend(theme.bg, theme.dim, 1.0);
            Some(blend(resting, theme.cube_light, opacity * 0.6))
        }
        Level::Lit => {
            let resting = blend(theme.bg, theme.cube_light, 0.55);
            Some(blend(resting, theme.cube_light, opacity))
        }
    }
}

/// The logo as text lines, with the sweep at the given time.
///
/// The lines are centered, and the background of the logo is left alone.
pub fn logo_lines(theme: &Theme, half_blocks: bool, secs: f32) -> Vec<Line<'static>> {
    let rows = LOGO_ROWS as f32;
    let cols = LOGO_COLS as f32;

    if !half_blocks {
        return LOGO
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let mut spans: Vec<Span<'static>> = Vec::new();
                for (column, character) in line.chars().enumerate() {
                    let cell = level(character);
                    // The sweep runs along the diagonal, from the bottom left to the top right.
                    let diag = (column as f32 + (rows - 1.0 - row as f32)) / (cols + rows);
                    let opacity = shine_opacity(diag, secs);
                    if let Some(color) = pixel_color(cell, theme, opacity) {
                        spans.push(Span::styled("██", Style::new().fg(color)));
                    } else {
                        // The background of the logo stays the background of the terminal.
                        spans.push(Span::styled("  ", Style::new()));
                    }
                }
                Line::from(spans).alignment(Alignment::Center)
            })
            .collect();
    }

    // Half blocks: two pixels share a row, so the glyph is one of the three half blocks.
    const UPPER: &str = "▀";
    const LOWER: &str = "▄";
    const FULL: &str = "█";

    let mut lines = Vec::new();
    let mut top = 0;
    while top < LOGO_ROWS {
        let bottom = top + 1;
        let upper_row: Vec<char> = LOGO[top].chars().collect();
        let lower_row: Vec<char> = LOGO
            .get(bottom)
            .map(|row| row.chars().collect())
            .unwrap_or_default();

        let mut spans: Vec<Span<'static>> = Vec::new();
        for (column, upper_character) in upper_row.iter().enumerate() {
            let upper = level(*upper_character);
            let lower = level(lower_row.get(column).copied().unwrap_or(' '));
            let diag = (column as f32 + (rows - 1.0 - top as f32)) / (cols + rows);
            let opacity = shine_opacity(diag, secs);

            let (glyph, cell) = match (upper, lower) {
                (Level::Dark, Level::Dark) => continue,
                (Level::Dark, _) => (LOWER, lower),
                (_, Level::Dark) => (UPPER, upper),
                _ => (FULL, Level::Lit),
            };
            if let Some(color) = pixel_color(cell, theme, opacity) {
                spans.push(Span::styled(glyph, Style::new().fg(color)));
            }
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
        top += 2;
    }
    lines
}

/// Draw the logo, centered in `area`.
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme, secs: f32, reduce_motion: bool) {
    // With reduced motion the logo rests: no sweep.
    let secs = if reduce_motion { 0.0 } else { secs };
    if area.width < PLAIN_TEXT_MIN_WIDTH {
        let text = Line::from(Span::styled(
            "SEEK",
            Style::new().fg(theme.cube_light).bold(),
        ))
        .alignment(Alignment::Center);
        let width = area.width.min(text.width() as u16);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        frame.render_widget(
            ratatui::widgets::Paragraph::new(text),
            Rect::new(x, area.y, width, 1),
        );
        return;
    }

    let half_blocks = area.height < FULL_BLOCK_HEIGHT;
    let lines = logo_lines(theme, half_blocks, secs);
    let width = lines
        .first()
        .map(|line| line.width() as u16)
        .unwrap_or(0)
        .min(area.width);
    let height = lines.len() as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    // The logo sits in the middle of the area, both ways.
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
    fn the_background_of_the_logo_is_not_drawn() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, false, 0.0);
        // Only the lit and the dim pixels are drawn; the rest is the background of the
        // terminal, so the mark has no block of colour around it.
        let drawn: usize = lines
            .iter()
            .map(|line| line.to_string().matches('█').count())
            .sum();
        let lit = LOGO
            .iter()
            .map(|row| row.matches('#').count())
            .sum::<usize>();
        let dim = LOGO
            .iter()
            .map(|row| row.matches('.').count())
            .sum::<usize>();
        // Every pixel is two columns wide.
        assert_eq!(
            drawn,
            (lit + dim) * 2,
            "only the pixels of the logo are drawn"
        );
    }

    #[test]
    fn the_letters_are_drawn() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, false, 0.0);
        let text: String = lines.iter().map(|line| line.to_string()).collect();
        assert!(text.contains('█'), "the logo is drawn");
    }

    #[test]
    fn the_lines_are_centered() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, false, 0.0);
        assert_eq!(lines[2].alignment, Some(Alignment::Center));
    }

    #[test]
    fn the_half_blocks_are_half_the_height() {
        let theme = Theme::default();
        let lines = logo_lines(&theme, true, 0.0);
        assert_eq!(lines.len(), LOGO_ROWS.div_ceil(2));
    }

    #[test]
    fn the_sweep_makes_the_logo_brighter() {
        let theme = Theme::default();
        // The resting logo and the one at the peak of the sweep have different colours.
        let resting = logo_lines(&theme, false, 0.0);
        let swept = logo_lines(&theme, false, 1.0);
        let color = |line: &Line<'static>| {
            line.spans
                .iter()
                .find_map(|span| span.style.fg)
                .expect("a colour")
        };
        assert_ne!(color(&resting[2]), color(&swept[2]));
    }
}

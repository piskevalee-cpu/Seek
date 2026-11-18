//! The thinking and loading animations (SPEC 10.3).
//!
//! Both are the same thing: a line of eight cells that are dark, with a streak of brightness
//! going through them. The streak travels along the line and wraps around: a new one starts at
//! the left when the old one reaches the right.
//!
//! The line replaces the cube grid of the first version of the specification; the specification
//! is updated with it.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::shimmer::{blend, twinkle, wave_brightness};
use crate::theme::Theme;

/// How many cells the line has.
pub const CELLS: u16 = 8;
/// How fast the wave travels.
pub const SPEED: f32 = 0.12;
/// How long one step lasts, in milliseconds.
pub const STEP_MS: u64 = 110;

/// The animated line.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spinner {
    frame: u64,
}

impl Spinner {
    pub fn new() -> Self {
        Spinner { frame: 0 }
    }

    /// Advance one step.
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The current frame.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// The cells of the animation, without a rail: used by the footer and the sidebar.
    pub fn cells_line(&self, theme: &Theme) -> Vec<Span<'static>> {
        cells_line_at(self.frame, theme)
    }

    /// The line of cells with the transcript rail, for the log strip.
    ///
    /// The rail is in front of the cells, the way the blocks of the transcript are.
    pub fn shimmer_line(&self, theme: &Theme) -> Line<'static> {
        let mut spans = Vec::with_capacity(CELLS as usize + 2);
        spans.push(Span::styled(
            crate::transcript::RAIL,
            Style::new().fg(theme.accent_thinking),
        ));
        spans.extend(self.cells_line(theme));
        Line::from(spans)
    }
}

/// The cells of the animation at a given frame.
pub fn cells_line_at(frame: u64, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(CELLS as usize);
    for index in 0..CELLS {
        // The travelling streak is joined by the same per-cell twinkle the logo uses, so the
        // thinking animation and the mark at the start of the session shimmer alike.
        let secs = frame as f32 * STEP_MS as f32 / 1000.0;
        let brightness = (wave_brightness(frame, index, CELLS, SPEED)
            + twinkle(index as usize, CELLS as usize, secs))
        .min(1.0);
        let color = blend(theme.bg, theme.cube_light, brightness);
        spans.push(Span::styled("█", Style::new().fg(color)));
    }
    spans
}

/// The static text used when `reduce_motion` is on.
pub const STATIC_THINKING: &str = "thinking...";
/// The static text used when `reduce_motion` is on.
pub const STATIC_LOADING: &str = "loading...";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_has_the_cells() {
        let theme = Theme::default();
        let line = Spinner::new().shimmer_line(&theme);
        // The rail is in front of the cubes.
        assert_eq!(line.width(), CELLS as usize + 2);
    }

    /// The colours of the cells of a line.
    fn cells(spinner: &Spinner, theme: &Theme) -> Vec<ratatui::style::Color> {
        spinner
            .shimmer_line(theme)
            .spans
            .iter()
            .map(|span| span.style.fg.expect("a colour"))
            .collect()
    }

    #[test]
    fn the_wave_moves_the_brightness() {
        let theme = Theme::default();
        let mut spinner = Spinner::new();
        let first = cells(&spinner, &theme);
        spinner.tick();
        assert_ne!(first, cells(&spinner, &theme));
    }

    #[test]
    fn the_cells_are_dark_at_the_rest() {
        let theme = Theme::default();
        // The streak starts at the left, so the cell in the middle is dark.
        let line = Spinner::new().shimmer_line(&theme);
        let color = line.spans[CELLS as usize / 2 + 1]
            .style
            .fg
            .expect("a colour");
        assert_eq!(color, theme.bg);
    }

    #[test]
    fn the_wave_lights_some_cells() {
        let theme = Theme::default();
        let line = Spinner::new().shimmer_line(&theme);
        let lit = line
            .spans
            .iter()
            .filter(|span| span.style.fg.expect("a colour") != theme.bg)
            .count();
        assert!(lit > 0, "the wave is somewhere on the line");
    }
}

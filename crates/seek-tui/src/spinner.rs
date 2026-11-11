//! The two spinners (SPEC 10.3).
//!
//! * **Thinking**: a 3x3 grid of cubes, numbered 123 / 456 / 789. The lit order is a clockwise
//!   spiral ending in the center: 1 2 3 6 9 8 7 4 5. At most three cubes are lit at once, one
//!   step per tick (about 110 ms).
//! * **Loading**: a row of five cubes alternating dark and light, shifting one position per tick.
//!
//! Every cube is two terminal columns of full blocks. `reduce_motion` replaces both with static
//! text.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// How long one spinner step lasts, in milliseconds.
pub const STEP_MS: u64 = 110;

/// The lit order of the thinking spinner, as indices into the 3x3 grid
/// (row major: 123 / 456 / 789).
const SPIRAL: [usize; 9] = [0, 1, 2, 5, 8, 7, 6, 3, 4];

/// The animated spinners.
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

    /// The cells that are lit at the current frame.
    ///
    /// The lit cubes are `path[f mod 9]`, `path[(f-1) mod 9]` and `path[(f-2) mod 9]`.
    pub fn lit_cells(&self) -> [usize; 3] {
        let f = self.frame as usize;
        [
            SPIRAL[f % 9],
            SPIRAL[(f + 9 - 1) % 9],
            SPIRAL[(f + 9 - 2) % 9],
        ]
    }

    /// The thinking spinner: three rows of three cubes.
    pub fn thinking_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let lit = self.lit_cells();
        let mut lines = Vec::with_capacity(3);
        for row in 0..3 {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(3);
            for column in 0..3 {
                let index = row * 3 + column;
                let style = if lit.contains(&index) {
                    Style::new().fg(theme.cube_light)
                } else {
                    Style::new().fg(theme.cube_dark)
                };
                spans.push(Span::styled("██", style));
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    /// The loading spinner: one row of five cubes.
    pub fn loading_line(&self, theme: &Theme) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
        for column in 0..5 {
            let lit = (column as u64 + self.frame).is_multiple_of(2);
            let style = if lit {
                Style::new().fg(theme.cube_light)
            } else {
                Style::new().fg(theme.cube_dark)
            };
            spans.push(Span::styled("██", style));
        }
        Line::from(spans)
    }
}

/// The static text used when `reduce_motion` is on.
pub const STATIC_THINKING: &str = "thinking...";
/// The static text used when `reduce_motion` is on.
pub const STATIC_LOADING: &str = "loading...";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spiral_visits_every_cell_once() {
        let mut seen = SPIRAL.to_vec();
        seen.sort_unstable();
        assert_eq!(seen, (0..9).collect::<Vec<_>>());
    }

    #[test]
    fn the_spiral_is_a_clockwise_walk() {
        // 1 2 3 6 9 8 7 4 5 in one-based cell numbers.
        let expected = [1, 2, 3, 6, 9, 8, 7, 4, 5];
        let actual: Vec<usize> = SPIRAL.iter().map(|i| i + 1).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn three_cubes_are_lit_at_a_time() {
        let spinner = Spinner::new();
        let lit = spinner.lit_cells();
        assert_eq!(lit.len(), 3);
        assert!(lit.iter().all(|cell| *cell < 9));
    }

    #[test]
    fn the_lit_cubes_move_on_every_step() {
        let mut spinner = Spinner::new();
        let first = spinner.lit_cells();
        spinner.tick();
        assert_ne!(first, spinner.lit_cells());
    }

    #[test]
    fn the_thinking_spinner_is_three_rows_of_six_columns() {
        let theme = Theme::default();
        let lines = Spinner::new().thinking_lines(&theme);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| line.width() == 6));
    }

    #[test]
    fn the_loading_spinner_is_one_row_of_ten_columns() {
        let theme = Theme::default();
        let line = Spinner::new().loading_line(&theme);
        assert_eq!(line.width(), 10);
    }
}

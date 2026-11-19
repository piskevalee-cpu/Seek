//! Gradient backgrounds for selected rows (SPEC 10.3).
//!
//! Ratatui has no gradient primitive, so a selected row is painted one cell at a time: the
//! background blends from the terminal background to the brand blue. The direction follows the
//! sidebar — left-to-right when it is on the left, right-to-left when it is on the right.

use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;

use crate::shimmer::blend;
use crate::theme::Theme;

/// Paint a row with a gradient background, padding it to `width` cells.
pub fn paint(
    spans: Vec<Span<'static>>,
    width: usize,
    from_left: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut column = 0usize;
    for span in spans {
        for ch in span.content.chars() {
            out.push(cell(ch, column, width, from_left, theme));
            column += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    while column < width {
        out.push(cell(' ', column, width, from_left, theme));
        column += 1;
    }
    out
}

/// One cell of the gradient. The text flips to the dark colour where the blue gets strong.
fn cell(ch: char, column: usize, width: usize, from_left: bool, theme: &Theme) -> Span<'static> {
    let position = if width <= 1 {
        0.0
    } else {
        column.min(width - 1) as f32 / (width - 1) as f32
    };
    let amount = if from_left { position } else { 1.0 - position };
    let bg = blend(theme.bg, theme.accent, amount);
    let fg = if amount > 0.58 { theme.bg } else { theme.text };
    Span::styled(ch.to_string(), Style::new().fg(fg).bg(bg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gradient_runs_in_the_chosen_direction() {
        let theme = Theme::default();
        let left = paint(vec![Span::raw("x")], 8, true, &theme);
        let right = paint(vec![Span::raw("x")], 8, false, &theme);
        assert_eq!(left.len(), 8);
        assert_ne!(left[0].style.bg, left[7].style.bg);
        assert_eq!(left[0].style.bg, right[7].style.bg);
    }
}

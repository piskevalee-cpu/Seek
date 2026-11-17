//! The shimmer: the travelling brightness that the logo and the animation share.
//!
//! The approach follows the grok-build pager (Apache-2.0): a wave of brightness travels along a
//! line, and the colour of every cell is the background blended toward a highlight by how much
//! of the wave is on it.

use ratatui::style::Color;

/// The brightness of a cell at `index` of a wave that is `width` cells wide.
///
/// The value is in `[0, 1]`: 0 is the resting colour, 1 is the full highlight.
///
/// The wave is a travelling pulse, not a `sin²`: it goes along the line and wraps around, so a
/// new streak starts at the left when the old one reaches the right. A `sin²` ping-pongs
/// instead, and the same cells light up again and again.
pub fn wave_brightness(tick: u64, index: u16, width: u16, speed: f32) -> f32 {
    let width = width.max(1) as f32;
    // The head of the streak travels along the line and wraps around.
    let head = (tick as f32 * speed).rem_euclid(1.0) * width;
    let position = index as f32;
    // The distance from the head, wrapped around the line: the streak comes back to the
    // left when it reaches the right.
    let distance = (position - head).rem_euclid(width);
    let distance = distance.min(width - distance);
    // The streak is a small triangle around the head.
    const HALF_WIDTH: f32 = 1.5;
    if distance >= HALF_WIDTH {
        return 0.0;
    }
    (1.0 - distance / HALF_WIDTH).powf(2.0)
}

/// The opacity of the shine at a diagonal position, at a point in time.
///
/// `diag` is the normalized position on the diagonal: 0 is the bottom left, 1 is the top right.
/// A raised-cosine band sweeps from the bottom left to the top right, and a gentle pulse
/// breathes underneath it.
pub fn shine_opacity(diag: f32, secs: f32) -> f32 {
    /// The half width of the band; wider means a more gradual falloff.
    const BAND: f32 = 0.38;
    /// The seconds of one sweep plus its rest.
    const CYCLE: f32 = 4.0;
    /// The part of the cycle that is spent sweeping.
    const SWEEP_FRAC: f32 = 0.32;
    /// The peak strength of the shine.
    const SHINE: f32 = 0.33;
    /// The amount of the global breathing.
    const PULSE: f32 = 0.06;
    /// The period of the breathing, in seconds.
    const PULSE_SECS: f32 = 5.0;

    let p = (secs % CYCLE) / CYCLE;
    let q = (p / SWEEP_FRAC).min(1.0);
    let band_pos = -BAND + q * (1.0 + 2.0 * BAND);
    let pulse = PULSE * (0.5 - 0.5 * (std::f32::consts::TAU * secs / PULSE_SECS).cos());

    let d = (diag - band_pos).abs();
    let shine = if d < BAND {
        0.5 * (1.0 + (std::f32::consts::PI * d / BAND).cos())
    } else {
        0.0
    };
    (pulse + SHINE * shine).clamp(0.0, 1.0)
}

/// Blend a colour toward another one.
///
/// The opacity is in `[0, 1]`: 0 keeps the base, 1 is the highlight.
pub fn blend(base: Color, highlight: Color, opacity: f32) -> Color {
    let opacity = opacity.clamp(0.0, 1.0);
    match (base, highlight) {
        (Color::Rgb(br, bg, bb), Color::Rgb(hr, hg, hb)) => {
            let mix = |b: u8, h: u8| -> u8 {
                let value = b as f32 + (h as f32 - b as f32) * opacity;
                value.round().clamp(0.0, 255.0) as u8
            };
            Color::Rgb(mix(br, hr), mix(bg, hg), mix(bb, hb))
        }
        (base, _) => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_stays_between_zero_and_one() {
        for tick in 0..200 {
            for index in 0..12 {
                let brightness = wave_brightness(tick, index, 12, 0.3);
                assert!((0.0..=1.0).contains(&brightness), "{brightness}");
            }
        }
    }

    #[test]
    fn the_wave_travels_along_the_line() {
        // The peak of the wave moves as the tick goes on.
        let peak = |tick: u64| {
            (0..12)
                .max_by_key(|index| (wave_brightness(tick, *index, 12, 0.3) * 1000.0) as u32)
                .unwrap()
        };
        assert_ne!(peak(0), peak(6));
    }

    #[test]
    fn the_streak_wraps_around_the_line() {
        // The head moves two cells per tick at a quarter speed, so it is at cell 6 after
        // three ticks and back at cell 0 after four.
        let width = 8;
        let speed = 0.25;
        let at_the_end = wave_brightness(3, 6, width, speed);
        let at_the_start = wave_brightness(4, 0, width, speed);
        assert!(at_the_end > 0.5, "the streak is at the end: {at_the_end}");
        assert!(
            at_the_start > 0.5,
            "the streak is back at the start: {at_the_start}"
        );
    }

    #[test]
    fn only_a_few_cells_are_lit() {
        // The streak is short: the line stays dark.
        let lit = (0..8)
            .filter(|index| wave_brightness(3, *index, 8, 0.25) > 0.0)
            .count();
        assert!(lit <= 4, "the streak is short: {lit} cells are lit");
    }

    #[test]
    fn the_shine_stays_between_zero_and_one() {
        for step in 0..100 {
            let secs = step as f32 * 0.1;
            for step in 0..20 {
                let diag = step as f32 / 19.0;
                let opacity = shine_opacity(diag, secs);
                assert!((0.0..=1.0).contains(&opacity), "{opacity}");
            }
        }
    }

    #[test]
    fn the_blend_moves_toward_the_highlight() {
        let base = Color::Rgb(0, 0, 0);
        let highlight = Color::Rgb(255, 255, 255);
        assert_eq!(blend(base, highlight, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(blend(base, highlight, 1.0), Color::Rgb(255, 255, 255));
        assert_eq!(blend(base, highlight, 0.5), Color::Rgb(128, 128, 128));
    }
}

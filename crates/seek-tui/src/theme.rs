//! The theme (SPEC 10.6).
//!
//! Tokens: `bg`, `cube_dark`, `cube_light`, `accent`, `dim`, `warn`, `error`. `cube_light` and
//! `accent` come from the logo (SPEC 12): they were sampled from `assets/logo.png` with
//! `tools/extract_logo.py`; see `docs/DECISIONS.md` (D-002).
//!
//! The `bg` token is the background of the terminal, not the green of the logo: a green terminal
//! is unreadable. The logo keeps its own background in the asset.
//!
//! Colors are never the only signal: the permission mode and the LOCAL tag are text.

use ratatui::style::Color;

/// The SEEK color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Terminal background.
    pub bg: Color,
    /// An unlit cube.
    pub cube_dark: Color,
    /// A lit cube, and the logo's foreground.
    pub cube_light: Color,
    /// The accent, used for highlights and the assistant's edge.
    pub accent: Color,
    /// Dimmed text.
    pub dim: Color,
    /// Warnings.
    pub warn: Color,
    /// Errors.
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            // The background of the terminal: dark on purpose.
            bg: rgb(0x14, 0x16, 0x17),
            cube_dark: rgb(0x2C, 0x2E, 0x2F),
            // Sampled from assets/logo.png.
            cube_light: rgb(0xB4, 0xB4, 0xB4),
            accent: rgb(0x22, 0xB1, 0x4C),
            dim: rgb(0x7C, 0x7D, 0x7E),
            warn: rgb(0xD9, 0xA4, 0x41),
            error: rgb(0xE0, 0x52, 0x52),
        }
    }
}

impl Theme {
    /// The theme, downgraded to the 256-color palette if the terminal cannot do truecolor.
    pub fn for_terminal() -> Self {
        let theme = Theme::default();
        match std::env::var("SEEK_COLOR").as_deref() {
            Ok("256") => theme.ansi256(),
            _ => theme,
        }
    }

    /// The same theme with every color mapped to the xterm 256-color palette.
    pub fn ansi256(&self) -> Self {
        Theme {
            bg: to_ansi256(self.bg),
            cube_dark: to_ansi256(self.cube_dark),
            cube_light: to_ansi256(self.cube_light),
            accent: to_ansi256(self.accent),
            dim: to_ansi256(self.dim),
            warn: to_ansi256(self.warn),
            error: to_ansi256(self.error),
        }
    }
}

/// Build a truecolor value.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

/// Map a truecolor value to the xterm 256-color palette.
///
/// Uses the standard 6x6x6 color cube plus the grayscale ramp.
pub fn to_ansi256(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let index = if r == g && g == b {
        // Grayscale ramp: 232..255 covers 8..238.
        if r < 8 {
            16
        } else if r > 238 {
            231
        } else {
            232 + ((r as u16 - 8) * 24 / 230) as u8
        }
    } else {
        let q = |v: u8| -> u16 {
            if v < 48 {
                0
            } else if v < 115 {
                1
            } else {
                ((v as u16 - 35) * 5 / 40).min(5)
            }
        };
        16 + 36 * q(r) as u8 + 6 * q(g) as u8 + q(b) as u8
    };
    Color::Indexed(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_maps_into_the_ramp() {
        assert_eq!(to_ansi256(Color::Rgb(0, 0, 0)), Color::Indexed(16));
        assert_eq!(to_ansi256(Color::Rgb(255, 255, 255)), Color::Indexed(231));
    }

    #[test]
    fn the_theme_survives_a_downgrade() {
        let theme = Theme::default().ansi256();
        assert!(matches!(theme.accent, Color::Indexed(_)));
        assert!(matches!(theme.bg, Color::Indexed(_)));
    }

    #[test]
    fn indexed_colors_are_left_alone() {
        assert_eq!(to_ansi256(Color::Indexed(42)), Color::Indexed(42));
        assert_eq!(to_ansi256(Color::Reset), Color::Reset);
    }
}

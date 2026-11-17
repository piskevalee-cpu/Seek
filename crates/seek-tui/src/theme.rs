//! The theme (SPEC 10.6).
//!
//! The palette is the tokyonight of the grok-build pager (Apache-2.0): the background is the
//! storm blue, the text the pale foreground, and the accents the blue of the user and the
//! magenta of the model. There is no green in the interface.
//!
//! Tokens: `bg`, `surface`, `cube_dark`, `cube_light`, `accent`, `accent_user`,
//! `accent_assistant`, `accent_thinking`, `accent_tool`, `text`, `text_dim`, `dim`, `warn`,
//! `error`, `border`, `border_active`.
//!
//! Colors are never the only signal: the permission mode and the LOCAL tag are text. The
//! LOCAL tag is only in the model picker: the chat says which model it is, and that is
//! enough.

use ratatui::style::Color;

/// The SEEK color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Terminal background.
    pub bg: Color,
    /// The surface of a band: the message of the user, the box of the prompt.
    pub surface: Color,
    /// An unlit cube.
    pub cube_dark: Color,
    /// A lit cube, and the logo's foreground.
    pub cube_light: Color,
    /// The accent, used for the highlights and the border of the prompt.
    pub accent: Color,
    /// The rail of the message of the user.
    pub accent_user: Color,
    /// The rail of the answer of the model.
    pub accent_assistant: Color,
    /// The rail of the reasoning.
    pub accent_thinking: Color,
    /// The rail of a tool.
    pub accent_tool: Color,
    /// The colour of the text.
    pub text: Color,
    /// The colour of the text that is not the main one.
    pub text_dim: Color,
    /// Dimmed text.
    pub dim: Color,
    /// The border of the prompt box.
    pub border: Color,
    /// The border of the prompt box when it has the focus.
    pub border_active: Color,
    /// Warnings.
    pub warn: Color,
    /// Errors.
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            // The tokyonight palette of the grok-build pager (Apache-2.0).
            bg: rgb(0x24, 0x28, 0x3B),
            surface: rgb(0x29, 0x2E, 0x42),
            cube_dark: rgb(0x3B, 0x42, 0x61),
            cube_light: rgb(0xC0, 0xCA, 0xF5),
            // The blue of the palette: there is no green in the interface.
            accent: rgb(0x7A, 0xA2, 0xF7),
            accent_user: rgb(0x7A, 0xA2, 0xF7),
            accent_assistant: rgb(0xBB, 0x9A, 0xF7),
            accent_thinking: rgb(0x3B, 0x42, 0x61),
            accent_tool: rgb(0x73, 0x7A, 0xA2),
            text: rgb(0xC0, 0xCA, 0xF5),
            text_dim: rgb(0xA9, 0xB1, 0xD6),
            dim: rgb(0x56, 0x5F, 0x89),
            border: rgb(0x32, 0x3E, 0x64),
            border_active: rgb(0x4B, 0x5C, 0x8C),
            warn: rgb(0xE0, 0xAF, 0x68),
            error: rgb(0xF7, 0x76, 0x8E),
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
            surface: to_ansi256(self.surface),
            cube_dark: to_ansi256(self.cube_dark),
            cube_light: to_ansi256(self.cube_light),
            accent: to_ansi256(self.accent),
            accent_user: to_ansi256(self.accent_user),
            accent_assistant: to_ansi256(self.accent_assistant),
            accent_thinking: to_ansi256(self.accent_thinking),
            accent_tool: to_ansi256(self.accent_tool),
            text: to_ansi256(self.text),
            text_dim: to_ansi256(self.text_dim),
            dim: to_ansi256(self.dim),
            border: to_ansi256(self.border),
            border_active: to_ansi256(self.border_active),
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

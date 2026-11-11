//! The footer (SPEC 10.3).
//!
//! A mode chip (`READ`, `ASK` or `YOLO`, colored per mode), the model name with a `LOCAL` tag
//! when the model is local, then the reasoning chip (hidden when the model has no levels), then
//! the working directory and git branch right-aligned.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// The permission mode (SPEC 9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Read,
    Ask,
    Yolo,
}

impl PermissionMode {
    /// The text of the chip.
    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::Read => "READ",
            PermissionMode::Ask => "ASK",
            PermissionMode::Yolo => "YOLO",
        }
    }

    /// The color of the chip.
    pub fn color(self, theme: &Theme) -> Color {
        match self {
            PermissionMode::Read => theme.dim,
            PermissionMode::Ask => theme.warn,
            PermissionMode::Yolo => theme.error,
        }
    }

    /// The next mode, cycling read -> ask -> yolo.
    pub fn next(self) -> Self {
        match self {
            PermissionMode::Read => PermissionMode::Ask,
            PermissionMode::Ask => PermissionMode::Yolo,
            PermissionMode::Yolo => PermissionMode::Read,
        }
    }
}

/// What the footer shows about the current model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChip {
    pub provider: String,
    pub model: String,
    /// Whether the model runs on this machine.
    pub local: bool,
}

/// The footer contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub mode: PermissionMode,
    pub model: ModelChip,
    /// The reasoning level, when the model has any.
    pub reasoning: Option<String>,
    pub cwd: String,
    pub branch: Option<String>,
}

/// Build the footer line.
pub fn footer_line(footer: &Footer, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!(" {} ", footer.mode.label()),
        Style::new()
            .fg(theme.bg)
            .bg(footer.mode.color(theme))
            .bold(),
    )];

    spans.push(Span::styled(
        format!(" {} ", footer.model.model),
        Style::new().fg(theme.cube_light),
    ));
    if footer.model.local {
        spans.push(Span::styled(
            " LOCAL ",
            Style::new().fg(theme.bg).bg(theme.accent).bold(),
        ));
    }
    spans.push(Span::styled(
        format!(" {}", footer.model.provider),
        Style::new().fg(theme.dim),
    ));

    if let Some(reasoning) = &footer.reasoning {
        spans.push(Span::styled(
            format!(" think: {reasoning}"),
            Style::new().fg(theme.dim),
        ));
    }

    let mut right = footer.cwd.clone();
    if let Some(branch) = &footer.branch {
        right.push(' ');
        right.push_str(branch);
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(right, Style::new().fg(theme.dim)));
    Line::from(spans)
}

/// Draw the footer.
pub fn render(frame: &mut Frame, area: Rect, footer: &Footer, theme: &Theme) {
    frame.render_widget(
        ratatui::widgets::Paragraph::new(footer_line(footer, theme)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn footer() -> Footer {
        Footer {
            mode: PermissionMode::Ask,
            model: ModelChip {
                provider: "fake".into(),
                model: "fake/demo".into(),
                local: false,
            },
            reasoning: None,
            cwd: "/tmp".into(),
            branch: Some("develop".into()),
        }
    }

    #[test]
    fn the_mode_chip_shows_the_mode_as_text() {
        let theme = Theme::default();
        let line = footer_line(&footer(), &theme).to_string();
        assert!(line.contains("ASK"));
    }

    #[test]
    fn the_local_tag_only_shows_for_local_models() {
        let theme = Theme::default();
        let mut footer = footer();
        assert!(!footer_line(&footer, &theme).to_string().contains("LOCAL"));
        footer.model.local = true;
        assert!(footer_line(&footer, &theme).to_string().contains("LOCAL"));
    }

    #[test]
    fn the_reasoning_chip_is_hidden_without_a_level() {
        let theme = Theme::default();
        let mut footer = footer();
        assert!(!footer_line(&footer, &theme).to_string().contains("think:"));
        footer.reasoning = Some("high".into());
        assert!(
            footer_line(&footer, &theme)
                .to_string()
                .contains("think: high")
        );
    }

    #[test]
    fn the_mode_cycles() {
        assert_eq!(PermissionMode::Read.next(), PermissionMode::Ask);
        assert_eq!(PermissionMode::Ask.next(), PermissionMode::Yolo);
        assert_eq!(PermissionMode::Yolo.next(), PermissionMode::Read);
    }
}

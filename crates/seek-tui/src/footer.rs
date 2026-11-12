//! The footer (SPEC 10.3).
//!
//! One line: the mode chip (`READ`, `ASK` or `YOLO`, colored per mode), the model name with a
//! `LOCAL` tag when the model is local, the reasoning chip (hidden when the model has no
//! levels), then the working directory and the git branch, and the metrics of the session.
//!
//! The metrics live here when the sidebar is closed. When the sidebar is open they are at the
//! bottom of the sidebar, and the footer shows the rest. Never both (SPEC 10.2).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// The permission mode (SPEC 9.4).
///
/// The mode itself lives in `seek-core`, because the agent loop uses it too.
pub use seek_core::PermissionMode;

/// The color of the mode chip.
pub fn mode_color(mode: PermissionMode, theme: &Theme) -> Color {
    match mode {
        PermissionMode::Read => theme.dim,
        PermissionMode::Ask => theme.warn,
        PermissionMode::Yolo => theme.error,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub mode: PermissionMode,
    pub model: ModelChip,
    /// The reasoning level, when the model has any.
    pub reasoning: Option<String>,
    pub cwd: String,
    pub branch: Option<String>,
    /// The metrics of the session, when the sidebar does not show them.
    pub metrics: Option<crate::metrics::Metrics>,
}

/// Build the footer line.
pub fn footer_line(footer: &Footer, theme: &Theme, width: u16) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!(" {} ", footer.mode.label()),
        Style::new()
            .fg(theme.bg)
            .bg(mode_color(footer.mode, theme))
            .bold(),
    )];

    spans.push(Span::styled(
        format!(" {} ", footer.model.model),
        Style::new().fg(theme.text),
    ));
    // The provider is named when the model is not local. The LOCAL tag is only in the model
    // picker: the chat says which model it is, and that is enough (SPEC 10.2).
    if !footer.model.local {
        spans.push(Span::styled(
            format!(" {}", footer.model.provider),
            Style::new().fg(theme.dim),
        ));
    }

    if let Some(reasoning) = &footer.reasoning {
        spans.push(Span::styled(
            format!(" think: {reasoning}"),
            Style::new().fg(theme.dim),
        ));
    }

    // The working directory and the branch. The path is short: the home is a `~` and only
    // the last two parts are kept, so the line has room for the metrics.
    let mut place = short_path(&footer.cwd);
    if let Some(branch) = &footer.branch {
        place.push(' ');
        place.push_str(branch);
    }
    spans.push(Span::styled(
        format!(" {place}"),
        Style::new().fg(theme.dim),
    ));

    // The metrics of the session, when the sidebar does not show them. They are dropped
    // from the right when the line is too narrow: the mode and the place come first.
    if let Some(metrics) = &footer.metrics {
        let used: u16 = spans.iter().map(|span| span.width() as u16).sum();
        if used < width {
            let room = width - used;
            spans.push(Span::styled(" ", Style::new()));
            spans.extend(metrics.compact_line(theme, room.saturating_sub(1)).spans);
        }
    }
    Line::from(spans)
}

/// The path of a folder, short enough for the footer.
///
/// The home is a `~` and only the last two parts are kept: the footer line has room for the
/// mode, the model and the metrics.
pub fn short_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() <= 2 {
        return path;
    }
    let tail = parts[parts.len() - 2..].join("/");
    if parts[0] == "~" {
        format!("~/{tail}")
    } else {
        tail
    }
}

/// Draw the footer.
pub fn render(frame: &mut Frame, area: Rect, footer: &Footer, theme: &Theme) {
    frame.render_widget(
        ratatui::widgets::Paragraph::new(footer_line(footer, theme, area.width)),
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
            metrics: None,
        }
    }

    #[test]
    fn the_mode_chip_shows_the_mode_as_text() {
        let theme = Theme::default();
        let line = footer_line(&footer(), &theme, 200).to_string();
        assert!(line.contains("ASK"));
    }

    #[test]
    fn the_local_tag_is_not_in_the_chat() {
        // The LOCAL tag is only in the model picker: the chat names the model.
        let theme = Theme::default();
        let mut footer = footer();
        footer.model.local = true;
        assert!(
            !footer_line(&footer, &theme, 200)
                .to_string()
                .contains("LOCAL")
        );
    }

    #[test]
    fn the_reasoning_chip_is_hidden_without_a_level() {
        let theme = Theme::default();
        let mut footer = footer();
        assert!(
            !footer_line(&footer, &theme, 200)
                .to_string()
                .contains("think:")
        );
        footer.reasoning = Some("high".into());
        assert!(
            footer_line(&footer, &theme, 200)
                .to_string()
                .contains("think: high")
        );
    }

    #[test]
    fn the_metrics_are_in_the_footer_when_the_sidebar_is_closed() {
        let theme = Theme::default();
        let mut footer = footer();
        footer.metrics = Some(crate::metrics::Metrics {
            context: seek_core::context::ContextWindow::new(32_000).with_usage(12_400, 0),
            ..Default::default()
        });
        let line = footer_line(&footer, &theme, 200).to_string();
        assert!(line.contains("12.4k"), "{line}");
        assert!(
            line.contains("/tmp"),
            "the place is in the same line: {line}"
        );
        assert!(line.contains("ASK"), "{line}");
    }

    #[test]
    fn the_path_is_short() {
        assert_eq!(short_path("/tmp"), "/tmp");
        assert_eq!(short_path("/a/b/c/d"), "c/d");
    }

    #[test]
    fn the_mode_cycles() {
        assert_eq!(PermissionMode::Read.next(), PermissionMode::Ask);
        assert_eq!(PermissionMode::Ask.next(), PermissionMode::Yolo);
        assert_eq!(PermissionMode::Yolo.next(), PermissionMode::Read);
    }

    #[test]
    fn every_mode_has_a_color() {
        let theme = Theme::default();
        for mode in [
            PermissionMode::Read,
            PermissionMode::Ask,
            PermissionMode::Yolo,
        ] {
            assert_ne!(mode_color(mode, &theme), theme.bg);
        }
    }
}

//! The sidebar (SPEC 10.3).
//!
//! Sections in order: Sessions (title and relative time, current project first, Enter resumes),
//! Processes (PID, tag, command, uptime; `x` kills the selected one), then Metrics pinned to the
//! bottom.
//!
//! Milestone v0.1 ships the shell: the sections exist, but sessions and processes are still empty
//! because the agent and the local supervisor land in v0.3 and v0.4.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::gradient;
use crate::metrics::Metrics;
use crate::spinner::cells_line_at;
use crate::theme::Theme;

/// How many sessions the sidebar shows; the full list lives in `/sessions`.
pub const SIDEBAR_SESSIONS: usize = 3;

/// The width of the sidebar.
pub const SIDEBAR_WIDTH: u16 = 32;

/// The sidebar state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sidebar {
    pub open: bool,
    /// Sessions, as (title, relative time).
    pub sessions: Vec<(String, String)>,
    /// Processes, as (pid, tag, command, uptime).
    pub processes: Vec<(u32, String, String, String)>,
    /// The index of the selected session.
    pub selected_session: usize,
    /// The index of the selected process.
    pub selected_process: usize,
    /// Whether the model is working; the animation then sits in the metrics section.
    pub thinking: bool,
    /// The frame of the thinking animation.
    pub frame: u64,
}

impl Sidebar {
    /// Open or close the sidebar.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Draw the sidebar.
///
/// The metrics are pinned to the bottom (SPEC 10.3).
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    sidebar: &Sidebar,
    theme: &Theme,
    metrics: &Metrics,
    sessions: &[seek_core::SessionInfo],
    selected_session: usize,
    from_left: bool,
) {
    let block = ratatui::widgets::Block::bordered()
        .border_style(Style::new().fg(theme.cube_dark))
        .title(Span::styled(" SEEK ", Style::new().fg(theme.dim)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Sessions",
        Style::new().fg(theme.accent).bold(),
    )));
    if sessions.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::new().fg(theme.dim).italic(),
        )));
    } else {
        let now = std::time::SystemTime::now();
        for (index, session) in sessions.iter().take(SIDEBAR_SESSIONS).enumerate() {
            let title = if session.title.is_empty() {
                "(no title)".to_string()
            } else {
                session.title.clone()
            };
            let mut spans = vec![
                Span::styled("  ", Style::new()),
                Span::styled(title, Style::new().fg(theme.cube_light)),
                Span::styled(
                    format!("  {}", session.age(now)),
                    Style::new().fg(theme.dim),
                ),
            ];
            if index == selected_session {
                spans = gradient::paint(spans, inner.width as usize, from_left, theme);
            }
            lines.push(Line::from(spans));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Processes",
        Style::new().fg(theme.accent).bold(),
    )));
    if sidebar.processes.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none",
            Style::new().fg(theme.dim).italic(),
        )));
    } else {
        for (pid, tag, command, uptime) in &sidebar.processes {
            lines.push(Line::from(vec![
                Span::styled(format!("  {pid}"), Style::new().fg(theme.cube_light)),
                Span::styled(format!(" {tag}"), Style::new().fg(theme.dim)),
                Span::styled(format!(" {command}"), Style::new().fg(theme.cube_light)),
                Span::styled(format!(" {uptime}"), Style::new().fg(theme.dim)),
            ]));
        }
    }

    // The metrics are pinned to the bottom of the sidebar.
    let mut metric_lines = metrics.sidebar_lines(theme);
    if sidebar.thinking {
        let mut spans = vec![
            Span::styled(" thinking ", Style::new().fg(theme.dim).italic()),
            Span::styled("  ", Style::new()),
        ];
        spans.extend(cells_line_at(sidebar.frame, theme));
        metric_lines.push(Line::from(spans));
    }
    let metrics_height = metric_lines.len() as u16;
    let list_height = inner.height.saturating_sub(metrics_height);

    frame.render_widget(
        ratatui::widgets::Paragraph::new(lines),
        Rect::new(inner.x, inner.y, inner.width, list_height),
    );
    if metrics_height > 0 {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(metric_lines),
            Rect::new(inner.x, inner.y + list_height, inner.width, metrics_height),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebar_starts_closed() {
        assert!(!Sidebar::default().open);
    }

    #[test]
    fn toggling_flips_the_state() {
        let mut sidebar = Sidebar::default();
        sidebar.toggle();
        assert!(sidebar.open);
        sidebar.toggle();
        assert!(!sidebar.open);
    }
}

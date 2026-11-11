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

use crate::theme::Theme;

/// The width of the sidebar.
pub const SIDEBAR_WIDTH: u16 = 30;

/// The sidebar state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sidebar {
    pub open: bool,
    /// Sessions, as (title, relative time).
    pub sessions: Vec<(String, String)>,
    /// Processes, as (pid, tag, command, uptime).
    pub processes: Vec<(u16, String, String, String)>,
    /// The index of the selected session.
    pub selected_session: usize,
    /// The index of the selected process.
    pub selected_process: usize,
}

impl Sidebar {
    /// Open or close the sidebar.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Draw the sidebar.
pub fn render(frame: &mut Frame, area: Rect, sidebar: &Sidebar, theme: &Theme) {
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
    if sidebar.sessions.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::new().fg(theme.dim).italic(),
        )));
    } else {
        for (title, age) in &sidebar.sessions {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::new()),
                Span::styled(title.clone(), Style::new().fg(theme.cube_light)),
                Span::styled(format!("  {age}"), Style::new().fg(theme.dim)),
            ]));
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

    frame.render_widget(
        ratatui::widgets::Paragraph::new(lines),
        Rect::new(inner.x, inner.y, inner.width, inner.height),
    );
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

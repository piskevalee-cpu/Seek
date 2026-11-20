//! The sidebar (SPEC 10.3).
//!
//! Three stacked blocks, chosen in `/settings`: sessions, hardware, processes and widgets. Block 1
//! is the top third, block 2 the middle, block 3 the bottom. The context bar, the thinking line and
//! the speed are not here: when the sidebar is open they sit under the prompt box.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::gradient;
use crate::hardware;
use crate::theme::Theme;
use crate::vitals::Vitals;

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
    /// Whether the model is working; the animation then sits under the prompt.
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

/// Draw one block into its slot.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    sidebar: &Sidebar,
    theme: &Theme,
    sessions: &[seek_core::SessionInfo],
    selected_session: usize,
    from_left: bool,
    blocks: [&str; 3],
    widgets: &[seek_core::WidgetOutput],
    vitals: &Vitals,
) {
    let block = ratatui::widgets::Block::bordered()
        .border_style(Style::new().fg(theme.cube_dark))
        .title(Span::styled(" SEEK ", Style::new().fg(theme.dim)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Three equal slots; the odd row goes to the top one.
    let total = inner.height as usize;
    let mut heights = [total / 3, total / 3, total / 3];
    heights[0] += total % 3;
    let mut y = inner.y;
    for (slot, height) in heights.into_iter().enumerate() {
        let slot_area = Rect::new(inner.x, y, inner.width, height as u16);
        let lines = block_lines(
            blocks[slot],
            sidebar,
            theme,
            sessions,
            selected_session,
            from_left,
            widgets,
            vitals,
        );
        frame.render_widget(ratatui::widgets::Paragraph::new(lines), slot_area);
        y += height as u16;
    }
}

/// The lines of one block.
#[allow(clippy::too_many_arguments)]
fn block_lines(
    name: &str,
    sidebar: &Sidebar,
    theme: &Theme,
    sessions: &[seek_core::SessionInfo],
    selected_session: usize,
    from_left: bool,
    widgets: &[seek_core::WidgetOutput],
    vitals: &Vitals,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match name {
        "sessions" => {
            lines.push(header("Sessions", theme));
            if sessions.is_empty() {
                lines.push(empty("none yet", theme));
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
                        spans =
                            gradient::paint(spans, SIDEBAR_WIDTH as usize - 2, from_left, theme);
                    }
                    lines.push(Line::from(spans));
                }
            }
        }
        "hardware" => {
            lines.push(header("Hardware", theme));
            lines.extend(hardware::lines(vitals, theme));
        }
        "processes" => {
            lines.push(header("Processes", theme));
            if sidebar.processes.is_empty() {
                lines.push(empty("none", theme));
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
        }
        "widgets" => {
            lines.push(header("Widgets", theme));
            if widgets.is_empty() {
                lines.push(empty("none", theme));
            } else {
                for widget in widgets.iter().take(3) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", widget.title),
                        Style::new().fg(theme.cube_light),
                    )));
                    for line in widget.lines.iter().take(3) {
                        lines.push(Line::from(Span::styled(
                            format!("    {line}"),
                            Style::new().fg(theme.dim),
                        )));
                    }
                }
            }
        }
        _ => {}
    }
    lines
}

fn header(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::new().fg(theme.accent).bold(),
    ))
}

fn empty(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::new().fg(theme.dim).italic(),
    ))
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

    #[test]
    fn each_block_has_its_own_header() {
        let theme = Theme::default();
        let sidebar = Sidebar::default();
        let vitals = Vitals::default();
        for (name, title) in [
            ("sessions", "Sessions"),
            ("hardware", "Hardware"),
            ("processes", "Processes"),
            ("widgets", "Widgets"),
        ] {
            let lines = block_lines(name, &sidebar, &theme, &[], 0, true, &[], &vitals);
            assert_eq!(lines[0].to_string(), title);
        }
    }
}

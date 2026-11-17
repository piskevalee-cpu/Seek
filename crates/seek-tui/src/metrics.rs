//! The METRICS block (SPEC 10.2, 10.3).
//!
//! The rows are, in this order: the context bar with the counts; the speed and the think time;
//! the rate limit, if the provider reported one; and the CPU, the memory, the GPU and the video
//! memory.
//!
//! The block has two forms: the sidebar form, where every row has its own line, and the
//! one-line form under the prompt bar, where the elements are dropped from the right when the
//! terminal is narrow.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use seek_core::context::{ContextWindow, Level, TurnStats};

use crate::theme::Theme;
use crate::vitals::Vitals;

/// How many cubes the context bar has.
pub const CUBES: usize = 10;

/// The partial blocks, from empty to full.
const PARTIALS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// The metrics of the current session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    /// The context window.
    pub context: ContextWindow,
    /// The numbers of the last turn.
    pub stats: TurnStats,
    /// The vitals of the machine.
    pub vitals: Option<Vitals>,
    /// The rate limit, when the provider reported one.
    pub rate_limit: Option<String>,
}

impl Metrics {
    /// The color of the context bar.
    pub fn context_color(&self, theme: &Theme) -> Color {
        match self.context.level() {
            Level::Normal => theme.accent,
            Level::Warn => theme.warn,
            Level::High => theme.error,
        }
    }

    /// The context bar, with the counts behind it.
    pub fn context_line(&self, theme: &Theme) -> Line<'static> {
        let color = self.context_color(theme);
        let mut spans: Vec<Span<'static>> = Vec::new();

        // Ten cubes, each a tenth of the window.
        let filled = self.context.fraction() * CUBES as f32;
        for index in 0..CUBES {
            let remaining = filled - index as f32;
            let glyph = if remaining >= 1.0 {
                "█"
            } else if remaining > 0.0 {
                let step = (remaining * 8.0).round() as usize;
                PARTIALS[step.clamp(1, 7)]
            } else {
                " "
            };
            let style = if remaining > 0.0 {
                Style::new().fg(color)
            } else {
                Style::new().fg(theme.cube_dark)
            };
            spans.push(Span::styled(glyph, style));
        }

        spans.push(Span::styled(
            format!(" {} ", self.context.counts()),
            Style::new().fg(theme.cube_light),
        ));
        spans.push(Span::styled(
            format!("{:.0}%", self.context.fraction() * 100.0),
            Style::new().fg(color),
        ));
        Line::from(spans)
    }

    /// The speed and the think time.
    pub fn speed_line(&self, theme: &Theme) -> Line<'static> {
        let mut spans = vec![Span::styled(
            format!("{:.1} t/s", self.stats.tokens_per_second()),
            Style::new().fg(theme.cube_light),
        )];
        if let Some(think) = self.stats.think_time_text() {
            spans.push(Span::styled(
                format!("  think {think}"),
                Style::new().fg(theme.dim),
            ));
        }
        Line::from(spans)
    }

    /// The rate limit, when there is one.
    pub fn rate_limit_line(&self, theme: &Theme) -> Option<Line<'static>> {
        self.rate_limit
            .as_ref()
            .map(|limit| Line::from(Span::styled(limit.clone(), Style::new().fg(theme.warn))))
    }

    /// The vitals of the machine.
    pub fn vitals_line(&self, theme: &Theme) -> Line<'static> {
        let vitals = self.vitals.clone().unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!("cpu {}", vitals.cpu_text()),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                format!("  ram {}", vitals.ram_text()),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                format!("  gpu {}", vitals.gpu_text()),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                format!("  vram {}", vitals.vram_text()),
                Style::new().fg(theme.dim),
            ),
        ])
    }

    /// The short form of the vitals: the processor and the memory.
    ///
    /// The footer line has room for two figures, not for four.
    pub fn vitals_short(&self, theme: &Theme) -> Line<'static> {
        let vitals = self.vitals.clone().unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!(" cpu {}", vitals.cpu_text()),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                format!("  ram {}", vitals.ram_text()),
                Style::new().fg(theme.dim),
            ),
        ])
    }

    /// The rows of the sidebar form.
    pub fn sidebar_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = vec![self.context_line(theme), self.speed_line(theme)];
        if let Some(limit) = self.rate_limit_line(theme) {
            lines.push(limit);
        }
        lines.push(self.vitals_line(theme));
        lines
    }

    /// The short form, for the footer line.
    ///
    /// The bar of the context is too wide for the footer: the counts and the percentage are
    /// enough there. The vitals come last and are dropped when the line is narrow.
    pub fn compact_line(&self, theme: &Theme, width: u16) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let color = self.context_color(theme);
        spans.push(Span::styled(
            format!("{} ", self.context.counts()),
            Style::new().fg(theme.text),
        ));
        spans.push(Span::styled(
            format!("{}%", (self.context.fraction() * 100.0).round() as u16),
            Style::new().fg(color),
        ));

        let speed = self.speed_line(theme);
        let limit = self.rate_limit_line(theme);
        let vitals = self.vitals_short(theme);

        let used: u16 = spans.iter().map(|span| span.width() as u16).sum();
        let speed_width = speed.width() as u16;
        let limit_width = limit.as_ref().map(|line| line.width() as u16).unwrap_or(0);
        let vitals_width = vitals.width() as u16;

        if used + speed_width + limit_width + vitals_width <= width {
            spans.extend(speed.spans);
            if let Some(limit) = limit {
                spans.extend(limit.spans);
            }
            spans.extend(vitals.spans);
        } else if used + speed_width + limit_width <= width {
            spans.extend(speed.spans);
            if let Some(limit) = limit {
                spans.extend(limit.spans);
            }
        } else if used + speed_width <= width {
            spans.extend(speed.spans);
        }
        Line::from(spans)
    }

    /// The one-line form, dropping the elements from the right when it is narrow.
    ///
    /// The order is the same as in the sidebar: the context bar, the speed, the rate limit and
    /// the vitals.
    pub fn one_line(&self, theme: &Theme, width: u16) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(self.context_line(theme).spans);

        let speed = self.speed_line(theme);
        let limit = self.rate_limit_line(theme);
        let vitals = self.vitals_line(theme);

        let used = spans.iter().map(|span| span.width() as u16).sum::<u16>();
        let speed_width = speed.width() as u16;
        let limit_width = limit.as_ref().map(|line| line.width() as u16).unwrap_or(0);
        let vitals_width = vitals.width() as u16;

        if used + speed_width + limit_width + vitals_width <= width {
            spans.extend(speed.spans);
            if let Some(limit) = limit {
                spans.extend(limit.spans);
            }
            spans.extend(vitals.spans);
        } else if used + speed_width + limit_width <= width {
            spans.extend(speed.spans);
            if let Some(limit) = limit {
                spans.extend(limit.spans);
            }
        } else if used + speed_width <= width {
            spans.extend(speed.spans);
        }
        Line::from(spans)
    }
}

/// Draw the metrics in the sidebar form.
pub fn render_sidebar(frame: &mut Frame, area: Rect, metrics: &Metrics, theme: &Theme) {
    let lines = metrics.sidebar_lines(theme);
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
}

/// Draw the metrics in the one-line form.
pub fn render_one_line(frame: &mut Frame, area: Rect, metrics: &Metrics, theme: &Theme) {
    let line = metrics.one_line(theme, area.width);
    frame.render_widget(ratatui::widgets::Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use seek_core::context::TurnStats;

    fn metrics() -> Metrics {
        Metrics {
            context: ContextWindow::new(32_000).with_usage(12_400, 0),
            stats: TurnStats {
                output_tokens: 120,
                generation_time: std::time::Duration::from_secs(2),
                think_time: std::time::Duration::from_millis(400),
                had_reasoning: true,
            },
            vitals: Some(Vitals {
                cpu: Some(12.0),
                ..Vitals::default()
            }),
            rate_limit: None,
        }
    }

    #[test]
    fn the_context_line_shows_the_counts_and_the_percentage() {
        let theme = Theme::default();
        let line = metrics().context_line(&theme).to_string();
        assert!(line.contains("12.4k / 32.0k"));
        assert!(line.contains("39%"));
    }

    #[test]
    fn the_bar_has_ten_cubes() {
        let theme = Theme::default();
        let line = metrics().context_line(&theme);
        // The bar is the first run of cube characters.
        let cubes: usize = line
            .spans
            .iter()
            .take_while(|span| {
                span.content
                    .chars()
                    .all(|c| c == '█' || c == ' ' || PARTIALS.contains(&c.to_string().as_str()))
            })
            .map(|span| span.width())
            .sum();
        assert_eq!(cubes, CUBES);
    }

    #[test]
    fn the_bar_is_amber_at_eighty_percent() {
        let theme = Theme::default();
        let mut metrics = metrics();
        metrics.context = ContextWindow::new(1000).with_usage(850, 0);
        assert_eq!(metrics.context_color(&theme), theme.warn);
        metrics.context = ContextWindow::new(1000).with_usage(960, 0);
        assert_eq!(metrics.context_color(&theme), theme.error);
    }

    #[test]
    fn the_speed_line_shows_the_think_time() {
        let theme = Theme::default();
        let line = metrics().speed_line(&theme).to_string();
        assert!(line.contains("60.0 t/s"));
        assert!(line.contains("think 0.4s"));
    }

    #[test]
    fn the_rate_limit_only_shows_when_it_is_reported() {
        let theme = Theme::default();
        let mut metrics = metrics();
        assert!(metrics.rate_limit_line(&theme).is_none());
        metrics.rate_limit = Some("rate limit 50 rpm".into());
        assert!(metrics.rate_limit_line(&theme).is_some());
    }

    #[test]
    fn the_sidebar_has_one_row_per_element() {
        let theme = Theme::default();
        let lines = metrics().sidebar_lines(&theme);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn the_one_line_form_drops_the_elements_when_it_is_narrow() {
        let theme = Theme::default();
        let metrics = metrics();
        let wide = metrics.one_line(&theme, 200).to_string();
        assert!(wide.contains("t/s"));
        assert!(wide.contains("cpu"));

        let narrow = metrics.one_line(&theme, 20).to_string();
        assert!(!narrow.contains("t/s"));
        assert!(narrow.contains("12.4k / 32.0k"));
    }
}

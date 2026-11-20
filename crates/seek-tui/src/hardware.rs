//! The hardware block of the sidebar (SPEC 10.3).
//!
//! An htop-like view: every core gets a bar that fills with a gradient, then memory, temperature
//! sensors and the GPU. The data comes from the 1 Hz sampler in `vitals`.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::shimmer::blend;
use crate::theme::Theme;
use crate::vitals::{Vitals, format_bytes};

const WIDTH: usize = 12;
const PARTIALS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// One bar cell: empty cubes are dark, filled cubes shade from accent to warn.
pub fn bar(percent: f32, theme: &Theme) -> Vec<Span<'static>> {
    let filled = (percent.clamp(0.0, 100.0) / 100.0) * WIDTH as f32;
    (0..WIDTH)
        .map(|index| {
            let remaining = filled - index as f32;
            if remaining <= 0.0 {
                return Span::styled("░", Style::new().fg(theme.cube_dark));
            }
            let color: Color = if index + 1 >= WIDTH {
                theme.warn
            } else {
                blend(theme.accent, theme.warn, index as f32 / WIDTH as f32)
            };
            let glyph = if remaining >= 1.0 {
                "█"
            } else {
                PARTIALS[((remaining * 8.0).round() as usize).clamp(1, 7)]
            };
            Span::styled(glyph, Style::new().fg(color))
        })
        .collect()
}

/// The lines of the hardware block, for a 30-column sidebar.
pub fn lines(vitals: &Vitals, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let cpu = vitals.cpu.unwrap_or(0.0);
    let mut head = vec![
        Span::styled(" cpu ", Style::new().fg(theme.dim)),
        Span::styled(format!("{cpu:.0}%"), Style::new().fg(theme.cube_light)),
    ];
    head.push(Span::styled(" ", Style::new()));
    head.extend(bar(cpu, theme));
    out.push(Line::from(head));

    // All cores, with the same gradual fill as the context bar.
    for (index, usage) in vitals.cores.iter().take(8).enumerate() {
        let mut core = vec![Span::styled(
            format!("  {index:<2}"),
            Style::new().fg(theme.dim),
        )];
        core.extend(bar(*usage, theme));
        core.push(Span::styled(
            format!(" {usage:.0}%"),
            Style::new().fg(theme.dim),
        ));
        out.push(Line::from(core));
    }

    if let (Some(used), Some(total)) = (vitals.ram_used, vitals.ram_total) {
        let percent = used as f32 / total.max(1) as f32 * 100.0;
        let mut ram = vec![Span::styled(
            format!(" ram {} ", format_bytes(used)),
            Style::new().fg(theme.dim),
        )];
        ram.extend(bar(percent, theme));
        out.push(Line::from(ram));
    }
    if let Some((label, temp)) = vitals.temps.first() {
        out.push(Line::from(vec![
            Span::styled(format!(" {label} "), Style::new().fg(theme.dim)),
            Span::styled(format!("{temp:.0}°"), Style::new().fg(theme.warn)),
        ]));
    }
    out.push(Line::from(vec![
        Span::styled(" gpu ", Style::new().fg(theme.dim)),
        Span::styled(vitals.gpu_text(), Style::new().fg(theme.cube_light)),
        Span::styled("  vram ", Style::new().fg(theme.dim)),
        Span::styled(vitals.vram_text(), Style::new().fg(theme.cube_light)),
    ]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_fills_and_shades() {
        let theme = Theme::default();
        let empty = bar(0.0, &theme);
        let full = bar(100.0, &theme);
        assert_eq!(empty.len(), WIDTH);
        assert!(empty.iter().all(|span| span.content == "░"));
        assert!(full.iter().all(|span| span.content == "█"));
        assert_ne!(
            full.first().unwrap().style.fg,
            full.last().unwrap().style.fg
        );
    }

    #[test]
    fn the_hardware_block_shows_every_core() {
        let theme = Theme::default();
        let vitals = Vitals {
            cpu: Some(42.0),
            cores: vec![30.0, 60.0],
            temps: vec![("CPU".into(), 52.0)],
            ..Default::default()
        };
        let rendered: Vec<String> = lines(&vitals, &theme)
            .into_iter()
            .map(|line| line.to_string())
            .collect();
        assert!(rendered[0].contains("42%"));
        assert!(rendered.iter().any(|line| line.contains("52°")));
    }
}

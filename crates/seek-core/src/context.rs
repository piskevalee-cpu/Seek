//! Context accounting (SPEC 8).
//!
//! The window of a model is the context size it reports: the real `n_ctx` of a local server, or
//! the value of the catalog for a cloud model.
//!
//! The used tokens are the usage of the last response — that is the ground truth. Until a
//! response arrives, the figure is an estimate and is marked with a `~`.

use std::time::Duration;

use crate::message::History;

/// The fraction of the window at which the context bar turns amber.
pub const WARN_FRACTION: f32 = 0.80;
/// The fraction of the window at which the context bar turns red.
pub const HIGH_FRACTION: f32 = 0.95;

/// The state of the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextWindow {
    /// The size of the window, in tokens.
    pub total: u32,
    /// The tokens that are used.
    pub used: u32,
    /// Whether the used figure is an estimate.
    pub estimated: bool,
}

impl Default for ContextWindow {
    fn default() -> Self {
        ContextWindow {
            total: 0,
            used: 0,
            estimated: true,
        }
    }
}

impl ContextWindow {
    /// A window with a known size and no usage yet.
    pub fn new(total: u32) -> Self {
        ContextWindow {
            total,
            used: 0,
            estimated: true,
        }
    }

    /// The used tokens of the last response: the ground truth.
    pub fn with_usage(&self, prompt_tokens: u32, output_tokens: u32) -> Self {
        ContextWindow {
            total: self.total,
            used: prompt_tokens.saturating_add(output_tokens),
            estimated: false,
        }
    }

    /// The used tokens of an estimate.
    pub fn with_estimate(&self, used: u32) -> Self {
        ContextWindow {
            total: self.total,
            used,
            estimated: true,
        }
    }

    /// How much of the window is used, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f32 / self.total as f32).clamp(0.0, 1.0)
    }

    /// How full the window is.
    pub fn level(&self) -> Level {
        let fraction = self.fraction();
        if fraction >= HIGH_FRACTION {
            Level::High
        } else if fraction >= WARN_FRACTION {
            Level::Warn
        } else {
            Level::Normal
        }
    }

    /// Whether the history does not fit into the window.
    pub fn overflows(&self, needed: u32) -> bool {
        self.total > 0 && needed > self.total
    }

    /// The used and the total, as they are shown.
    pub fn counts(&self) -> String {
        let prefix = if self.estimated { "~" } else { "" };
        format!(
            "{prefix}{} / {}",
            format_tokens(self.used),
            format_tokens(self.total)
        )
    }
}

/// How full the context is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warn,
    High,
}

/// The numbers of one turn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnStats {
    /// The tokens the model produced.
    pub output_tokens: u32,
    /// How long the generation took.
    pub generation_time: Duration,
    /// The time from the first reasoning token to the first answer.
    pub think_time: Duration,
    /// Whether the model produced any reasoning.
    pub had_reasoning: bool,
}

impl TurnStats {
    /// The output speed in tokens per second.
    ///
    /// The reasoning tokens count: they are produced too.
    pub fn tokens_per_second(&self) -> f64 {
        let seconds = self.generation_time.as_secs_f64();
        if seconds <= 0.0 {
            return 0.0;
        }
        self.output_tokens as f64 / seconds
    }

    /// The think time, as it is shown.
    ///
    /// It is hidden when the model produced no reasoning.
    pub fn think_time_text(&self) -> Option<String> {
        if !self.had_reasoning {
            return None;
        }
        Some(format!("{:.1}s", self.think_time.as_secs_f64()))
    }
}

/// Estimate the tokens of a text.
///
/// The heuristic is four bytes per token, rounded up. It is only used where no tokenizer is
/// available; the real count comes from the usage of the response.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.len().div_ceil(4).max(1)
}

/// Estimate the tokens of a whole conversation.
pub fn estimate_history(messages: &[crate::message::Message]) -> usize {
    messages
        .iter()
        .map(|message| {
            message
                .blocks
                .iter()
                .map(|block| match block {
                    crate::message::Block::Text { text } => estimate_tokens(text),
                    crate::message::Block::Thinking { text, .. } => estimate_tokens(text),
                    crate::message::Block::ToolUse { input, .. } => {
                        estimate_tokens(&input.to_string())
                    }
                    crate::message::Block::ToolResult { content, .. } => estimate_tokens(content),
                })
                .sum::<usize>()
        })
        .sum()
}

/// The number of the oldest turns that fit into a window.
///
/// Used when the history is truncated after a model switch (SPEC 8).
pub fn turns_that_fit(history: &History, window: u32) -> usize {
    let window = window as usize;
    let mut used = 0;
    let mut count = 0;
    for message in history.messages.iter().rev() {
        let size = message
            .blocks
            .iter()
            .map(|block| match block {
                crate::message::Block::Text { text } => estimate_tokens(text),
                crate::message::Block::Thinking { text, .. } => estimate_tokens(text),
                crate::message::Block::ToolUse { input, .. } => estimate_tokens(&input.to_string()),
                crate::message::Block::ToolResult { content, .. } => estimate_tokens(content),
            })
            .sum::<usize>();
        if used + size > window {
            break;
        }
        used += size;
        count += 1;
    }
    count
}

/// Format a number of tokens: `12.4k` or `1.2M`.
pub fn format_tokens(tokens: u32) -> String {
    let tokens = tokens as f64;
    if tokens >= 1_000_000.0 {
        format!("{:.1}M", tokens / 1_000_000.0)
    } else if tokens >= 1_000.0 {
        format!("{:.1}k", tokens / 1_000.0)
    } else {
        format!("{tokens:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Block, Message, Role};

    #[test]
    fn the_window_knows_how_full_it_is() {
        let window = ContextWindow::new(1000);
        assert_eq!(window.fraction(), 0.0);

        let window = window.with_usage(800, 100);
        assert_eq!(window.used, 900);
        assert!((window.fraction() - 0.9).abs() < 0.001);
        assert_eq!(window.level(), Level::Warn);
        assert!(!window.estimated);
    }

    #[test]
    fn the_bar_turns_red_at_ninety_five_percent() {
        let window = ContextWindow::new(1000).with_usage(960, 0);
        assert_eq!(window.level(), Level::High);
    }

    #[test]
    fn an_estimate_is_marked_with_a_tilde() {
        let window = ContextWindow::new(1000).with_estimate(500);
        assert!(window.estimated);
        assert!(window.counts().starts_with('~'));
    }

    #[test]
    fn the_counts_are_readable() {
        let window = ContextWindow::new(200_000).with_usage(12_400, 0);
        assert_eq!(window.counts(), "12.4k / 200.0k");
    }

    #[test]
    fn the_speed_counts_the_reasoning_tokens() {
        let stats = TurnStats {
            output_tokens: 100,
            generation_time: Duration::from_secs(2),
            think_time: Duration::from_millis(500),
            had_reasoning: true,
        };
        assert_eq!(stats.tokens_per_second(), 50.0);
        assert_eq!(stats.think_time_text().as_deref(), Some("0.5s"));
    }

    #[test]
    fn the_think_time_is_hidden_without_reasoning() {
        let stats = TurnStats {
            output_tokens: 10,
            generation_time: Duration::from_secs(1),
            think_time: Duration::from_millis(500),
            had_reasoning: false,
        };
        assert_eq!(stats.think_time_text(), None);
    }

    #[test]
    fn the_history_is_estimated() {
        let mut history = History::new();
        history.push(Message::user("hello"));
        assert!(estimate_history(&history.messages) > 0);
        assert_eq!(estimate_history(&[]), 0);
    }

    #[test]
    fn the_oldest_turns_that_fit_are_counted() {
        let mut history = History::new();
        for n in 0..10 {
            history.push(Message::new(Role::User, vec![Block::text("x".repeat(400))]));
            let _ = n;
        }
        // Every message is about 100 tokens, so a window of 500 holds five of them.
        let fits = turns_that_fit(&history, 500);
        assert!((1..=5).contains(&fits), "{fits} turns fit");
    }

    #[test]
    fn the_format_of_the_tokens() {
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(12_400), "12.4k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }
}

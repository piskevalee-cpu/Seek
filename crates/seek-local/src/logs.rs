//! The log of the local server (SPEC 7.3, 10.3).
//!
//! The stdout and the stderr of the server go into a ring buffer and into `state/logs/`. The log
//! strip shows the last three lines.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

/// How many lines the buffer keeps.
pub const CAPACITY: usize = 200;

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub text: String,
    /// When the line arrived.
    pub at: Instant,
    /// Whether the line came from stderr.
    pub error: bool,
}

impl LogLine {
    /// A line that just arrived.
    pub fn new(text: impl Into<String>, error: bool) -> Self {
        LogLine {
            text: text.into(),
            at: Instant::now(),
            error,
        }
    }

    /// The age of the line.
    pub fn age(&self) -> std::time::Duration {
        self.at.elapsed()
    }
}

/// The ring buffer of the log.
#[derive(Debug, Clone)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    capacity: usize,
}

impl LogBuffer {
    /// A buffer that keeps `capacity` lines.
    pub fn new(capacity: usize) -> Self {
        LogBuffer {
            lines: VecDeque::new(),
            capacity,
        }
    }

    /// Add a line.
    pub fn push(&mut self, line: LogLine) {
        self.lines.push_back(line);
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
        }
    }

    /// The last lines, newest last.
    pub fn last(&self, count: usize) -> Vec<&LogLine> {
        let skip = self.lines.len().saturating_sub(count);
        self.lines.iter().skip(skip).collect()
    }

    /// How many lines the buffer holds.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Whether the log has been quiet for a while.
    ///
    /// The log strip hides after ten seconds of silence (SPEC 10.3).
    pub fn is_quiet(&self, after: std::time::Duration) -> bool {
        self.lines.back().is_none_or(|line| line.age() > after)
    }

    /// Write the buffer to a file, so the log survives a restart.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        for line in &self.lines {
            text.push_str(&line.text);
            text.push('\n');
        }
        std::fs::write(path, text)
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        LogBuffer::new(CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_keeps_the_last_lines() {
        let mut buffer = LogBuffer::new(3);
        for n in 0..5 {
            buffer.push(LogLine::new(format!("line {n}"), false));
        }
        assert_eq!(buffer.len(), 3);
        let lines: Vec<_> = buffer
            .last(2)
            .iter()
            .map(|line| line.text.clone())
            .collect();
        assert_eq!(lines, vec!["line 3", "line 4"]);
    }

    #[test]
    fn an_empty_buffer_is_quiet() {
        let buffer = LogBuffer::new(3);
        assert!(buffer.is_quiet(std::time::Duration::from_secs(10)));
    }

    #[test]
    fn the_log_is_written_to_a_file() {
        let dir = std::env::temp_dir().join("seek-log-test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut buffer = LogBuffer::new(10);
        buffer.push(LogLine::new("hello", false));
        buffer
            .write_to(&dir.join("logs").join("server.log"))
            .expect("written");
        let text = std::fs::read_to_string(dir.join("logs").join("server.log")).expect("read");
        assert_eq!(text, "hello\n");
    }
}

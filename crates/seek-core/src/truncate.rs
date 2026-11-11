//! Truncating the output of the tools (SPEC 9.2).
//!
//! Every tool output is capped: 200 lines or 16 KB, head and tail, with a marker that tells the
//! model how to fetch more.
//!
//! The approach follows the one of the grok-build tools (Apache-2.0): the cut always lands on a
//! character boundary, and a long line is wrapped instead of being dropped.

/// The maximum number of lines of tool output.
pub const MAX_LINES: usize = 200;
/// The maximum number of bytes of tool output.
pub const MAX_BYTES: usize = 16 * 1024;
/// How wide a line is wrapped at.
pub const WRAP_WIDTH: usize = 2_000;

/// The marker that tells the model the output was cut.
pub const TRUNCATION_MARKER: &str = "…";

/// Cut a string to at most `max_bytes` bytes, on a character boundary.
///
/// The cut never splits a character, so the result is always valid UTF-8.
pub fn truncate_str(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or("")
}

/// Wrap a long line, keeping every character.
///
/// A line that is too long is soft-wrapped instead of being cut, so the model sees all of it.
pub fn soft_wrap(line: &str, width: usize) -> String {
    if width == 0 || line.chars().count() <= width {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + line.len() / width);
    let mut on_this_line = 0;
    for character in line.chars() {
        if on_this_line >= width {
            out.push('\n');
            on_this_line = 0;
        }
        out.push(character);
        on_this_line += 1;
    }
    out
}

/// Cap the output of a tool.
///
/// The head and the tail are kept, so the model sees the beginning and the end. The marker says
/// how much was left out and how to fetch it.
pub fn cap_output(text: &str) -> String {
    cap_output_with_hint(text, "use read with offset and limit to see more")
}

/// Cap the output of a tool, with a hint that says how to fetch the rest.
pub fn cap_output_with_hint(text: &str, hint: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total_bytes = text.len();

    if lines.len() <= MAX_LINES && total_bytes <= MAX_BYTES {
        return text.to_string();
    }

    // Half of the budget for the head, half for the tail.
    let half_bytes = MAX_BYTES / 2;
    let half_lines = MAX_LINES / 2;

    let mut head: Vec<&str> = Vec::new();
    let mut head_bytes = 0;
    for line in &lines {
        if head_bytes + line.len() > half_bytes {
            break;
        }
        head.push(line);
        head_bytes += line.len();
    }

    let mut tail: Vec<&str> = Vec::new();
    let mut tail_bytes = 0;
    for line in lines.iter().rev() {
        if tail_bytes + line.len() > half_bytes {
            break;
        }
        tail.push(line);
        tail_bytes += line.len();
    }
    // The tail was collected from the end, so it is cut before it is put back in order.
    tail.truncate(half_lines);
    tail.reverse();
    head.truncate(half_lines);

    let kept_bytes = head_bytes + tail_bytes;
    let omitted_lines = lines.len().saturating_sub(head.len() + tail.len());
    let omitted_bytes = total_bytes.saturating_sub(kept_bytes);

    let mut out = String::new();
    for line in &head {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!(
        "{TRUNCATION_MARKER} [{omitted_lines} lines and {omitted_bytes} bytes omitted, {hint}] {TRUNCATION_MARKER}\n"
    ));
    for line in &tail {
        out.push_str(line);
        out.push('\n');
    }
    out.pop();
    out
}

/// Wrap the lines of an output that are too long.
pub fn wrap_long_lines(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&soft_wrap(line, width));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cut_lands_on_a_character_boundary() {
        // Four bytes per character: the cut in the middle of one would panic.
        let text = "ä".repeat(100);
        let cut = truncate_str(&text, 10);
        assert!(cut.len() <= 10);
        assert!(text.starts_with(cut));
    }

    #[test]
    fn a_short_output_is_untouched() {
        assert_eq!(cap_output("hello"), "hello");
    }

    #[test]
    fn a_long_output_keeps_the_head_and_the_tail() {
        let text = (0..1000)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let capped = cap_output(&text);
        assert!(capped.starts_with("line 0"));
        assert!(capped.contains("line 999"));
        assert!(capped.contains("omitted"));
        assert!(capped.len() < text.len());
    }

    #[test]
    fn the_cap_respects_the_byte_limit() {
        let text = "x".repeat(100_000);
        let capped = cap_output(&text);
        assert!(capped.len() <= MAX_BYTES + 200, "{} bytes", capped.len());
    }

    #[test]
    fn a_long_line_is_wrapped_and_kept() {
        let line = "y".repeat(5000);
        let wrapped = soft_wrap(&line, 100);
        assert_eq!(wrapped.lines().count(), 50);
        assert_eq!(wrapped.replace('\n', "").len(), 5000);
    }

    #[test]
    fn wrapping_keeps_every_line() {
        let text = "first\nsecond\nthird";
        assert_eq!(wrap_long_lines(text, 100), text);
    }
}

//! Server-sent events (SPEC 6).
//!
//! The Anthropic and OpenAI-compatible protocols both stream their answers as SSE. The decoder
//! is deliberately small: it turns a byte stream into events, and the protocol decoders turn
//! events into provider events.

use std::collections::BTreeMap;

/// One SSE event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if the server sent one.
    pub event: Option<String>,
    /// The `data:` field. Multiple data lines are joined with a newline.
    pub data: String,
}

impl SseEvent {
    /// An event with only data.
    pub fn data(data: impl Into<String>) -> Self {
        SseEvent {
            event: None,
            data: data.into(),
        }
    }
}

/// Turns a stream of text chunks into SSE events.
///
/// The decoder keeps whatever it could not use yet, so a chunk may split an event anywhere.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    pub fn new() -> Self {
        SseDecoder::default()
    }

    /// Feed a chunk of the stream and take out the events it completed.
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        // A chunk may split "\r\n" in half. The "\r" is only a line ending when a "\n" follows
        // it, so it is kept until the next chunk decides.
        if self.buffer.ends_with('\r') && chunk.starts_with('\n') {
            self.buffer.pop();
        }
        self.buffer.push_str(chunk);
        self.buffer = self.buffer.replace("\r\n", "\n");

        let mut events = Vec::new();
        while let Some(end) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..end + 2).collect();
            if let Some(event) = parse_block(&block) {
                events.push(event);
            }
        }
        events
    }

    /// Flush what is left when the stream ends.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let block = std::mem::take(&mut self.buffer);
        match parse_block(&block) {
            Some(event) => vec![event],
            None => Vec::new(),
        }
    }
}

/// Parse one SSE block.
fn parse_block(block: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data: Vec<String> = Vec::new();
    let mut fields: BTreeMap<&str, String> = BTreeMap::new();

    for line in block.lines() {
        if line.is_empty() || line.starts_with(':') {
            // A comment or an empty line.
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => event = Some(value.to_string()),
            "data" => data.push(value.to_string()),
            other => {
                fields.insert(other, value.to_string());
            }
        }
    }

    if event.is_none() && data.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_event_is_decoded() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("event: message\ndata: {\"a\":1}\n\n");
        assert_eq!(
            events,
            vec![SseEvent {
                event: Some("message".into()),
                data: "{\"a\":1}".into(),
            }]
        );
    }

    #[test]
    fn an_event_may_be_split_across_chunks() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push("event: mess").is_empty());
        assert!(decoder.push("age\ndata: {\"a\"").is_empty());
        let events = decoder.push(":1}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"a\":1}");
    }

    #[test]
    fn comments_are_ignored() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(": keep-alive\n\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn several_data_lines_are_joined() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: one\ndata: two\n\n");
        assert_eq!(events[0].data, "one\ntwo");
    }

    #[test]
    fn windows_line_endings_work() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: hello\r\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn a_chunk_may_split_a_crlf_in_half() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: hello\r");
        assert!(events.is_empty());
        let events = decoder.push("\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn the_end_of_the_stream_flushes_the_last_event() {
        let mut decoder = SseDecoder::new();
        // No blank line yet, so the event is not complete.
        assert!(decoder.push("data: hello\n").is_empty());
        let events = decoder.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }
}

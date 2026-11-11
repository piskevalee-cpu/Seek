//! The SSE parsers are tested against recorded fixtures (SPEC 14).

use std::fs;

use seek_llm::anthropic::AnthropicDecoder;
use seek_llm::openai::OpenAiDecoder;
use seek_llm::provider::ProviderEvent;
use seek_llm::sse::SseDecoder;

/// Read a recorded fixture.
fn fixture(name: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
    fs::read_to_string(format!("{path}{name}")).expect("the fixture exists")
}

/// Decode a whole fixture with the SSE decoder.
fn sse_events(text: &str) -> Vec<seek_llm::SseEvent> {
    let mut decoder = SseDecoder::new();
    let mut events = decoder.push(text);
    events.extend(decoder.finish());
    events
}

#[test]
fn the_anthropic_tool_call_fixture_is_decoded() {
    let events = sse_events(&fixture("anthropic_tool_use.sse"));
    assert_eq!(events.len(), 7);
    assert_eq!(events[0].event.as_deref(), Some("message_start"));

    let mut decoder = AnthropicDecoder::new();
    let mut decoded = Vec::new();
    for event in &events {
        decoded.extend(decoder.push(event).expect("the fixture decodes"));
    }

    // The tool call is emitted with its accumulated input.
    let call = decoded
        .iter()
        .find_map(|event| match event {
            ProviderEvent::ToolCall { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .expect("the fixture has a tool call");
    assert_eq!(call.0, "toolu_01A");
    assert_eq!(call.1, "read");
    assert_eq!(call.2["path"], "src/main.rs");

    // The usage is reported for the input and for the output.
    let usage: Vec<_> = decoded
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::Usage { input, output } => Some((*input, *output)),
            _ => None,
        })
        .collect();
    assert_eq!(usage, vec![(120, 0), (120, 45)]);

    // The stream ends.
    assert!(matches!(decoded.last(), Some(ProviderEvent::Done)));
}

#[test]
fn the_anthropic_text_fixture_is_decoded() {
    let events = sse_events(&fixture("anthropic_text.sse"));
    let mut decoder = AnthropicDecoder::new();
    let mut decoded = Vec::new();
    for event in &events {
        decoded.extend(decoder.push(event).expect("the fixture decodes"));
    }

    let thinking: String = decoded
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::ThinkingDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thinking, "The user wants a summary.");

    let text: String = decoded
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::TextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Here is the summary.");
}

#[test]
fn the_openai_fixture_is_decoded() {
    let events = sse_events(&fixture("openai_tool_use.sse"));
    assert_eq!(events.last().unwrap().data, "[DONE]");

    let mut decoder = OpenAiDecoder::new();
    let mut decoded = Vec::new();
    for event in &events {
        if event.data.trim() == "[DONE]" {
            decoded.extend(decoder.finish());
            continue;
        }
        decoded.extend(decoder.push(event).expect("the fixture decodes"));
    }

    let text: String = decoded
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::TextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Reading the file.");

    let call = decoded
        .iter()
        .find_map(|event| match event {
            ProviderEvent::ToolCall { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .expect("the fixture has a tool call");
    assert_eq!(call.0, "call_abc");
    assert_eq!(call.1, "read");
    assert_eq!(call.2["path"], "src/main.rs");

    let usage = decoded
        .iter()
        .find_map(|event| match event {
            ProviderEvent::Usage { input, output } => Some((*input, *output)),
            _ => None,
        })
        .expect("the fixture has usage");
    assert_eq!(usage, (31, 18));
}

#[test]
fn a_fixture_survives_being_split_into_arbitrary_chunks() {
    let text = fixture("anthropic_tool_use.sse");
    let mut decoder = AnthropicDecoder::new();
    let mut decoded = Vec::new();

    // One decoder for the whole stream: the chunks may split events anywhere.
    let mut sse = SseDecoder::new();
    for chunk in text.as_bytes().chunks(7) {
        let chunk = String::from_utf8_lossy(chunk).to_string();
        for event in sse.push(&chunk) {
            decoded.extend(decoder.push(&event).expect("the fixture decodes"));
        }
    }

    assert!(
        decoded
            .iter()
            .any(|event| matches!(event, ProviderEvent::ToolCall { .. }))
    );
    assert!(matches!(decoded.last(), Some(ProviderEvent::Done)));
}

//! The OpenAI-compatible provider (SPEC 6).
//!
//! `POST /chat/completions` with `stream: true` and `stream_options: { include_usage: true }`.
//! Tool call deltas are accumulated by index.
//!
//! The same provider serves every OpenAI-compatible host: OpenRouter, OpenAI itself and the
//! local llama.cpp server. llama.cpp puts reasoning in `delta.reasoning_content` when it runs with
//! `--reasoning-format deepseek`.

use std::collections::BTreeMap;

use std::error::Error;

use futures::StreamExt;
use futures::stream::BoxStream;
use seek_core::{Message, Role};
use serde_json::{Value, json};

use crate::provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};
use crate::sse::SseEvent;

/// A provider that talks to an OpenAI-compatible endpoint.
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    max_tokens: u32,
    /// Extra fields merged into every request body, e.g. `reasoning_effort`.
    extra_body: Value,
    /// The client side limits of the configuration (SPEC 6).
    limiter: std::sync::Mutex<crate::ratelimit::Limiter>,
}

impl OpenAiProvider {
    /// A provider for one model.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        max_tokens: u32,
    ) -> Self {
        OpenAiProvider {
            client: crate::client::a_client(),
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            max_tokens,
            extra_body: json!({}),
            limiter: std::sync::Mutex::new(crate::ratelimit::Limiter::new(
                crate::ratelimit::Limit::none(),
            )),
        }
    }

    /// The models the endpoint knows (SPEC 6).
    ///
    /// The entries of `providers.toml` override the discovered ones (the rule opencode uses).
    pub async fn list_models(&self) -> Vec<String> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let mut builder = self.client.get(&url);
        if let Some(key) = &self.api_key {
            builder = builder.header("authorization", format!("Bearer {key}"));
        }
        let Ok(response) = builder.send().await else {
            return Vec::new();
        };
        let Ok(value) = response.json::<Value>().await else {
            return Vec::new();
        };
        let Some(data) = value.get("data").and_then(Value::as_array) else {
            return Vec::new();
        };
        data.iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
            .collect()
    }

    /// The client side limits of the configuration (SPEC 6).
    pub fn with_limit(mut self, limit: crate::ratelimit::Limit) -> Self {
        self.limiter = std::sync::Mutex::new(crate::ratelimit::Limiter::new(limit));
        self
    }

    /// Merge extra fields into the request body.
    pub fn with_extra_body(mut self, extra: Value) -> Self {
        self.extra_body = extra;
        self
    }

    /// The URL of the chat completions endpoint.
    pub fn url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Build the request body.
    pub fn build_body(&self, request: &ChatRequest) -> Value {
        let messages = encode_messages(&request.messages, request.system.as_deref());

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_tokens": request.max_tokens.unwrap_or(self.max_tokens),
        });

        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": serde_json::from_str::<Value>(&tool.parameters)
                                .unwrap_or(Value::Null),
                        }
                    })
                })
                .collect();
            body["tools"] = Value::Array(tools);
        }

        if let (Value::Object(extra), Value::Object(body)) = (&self.extra_body, &mut body) {
            for (key, value) in extra {
                body.insert(key.clone(), value.clone());
            }
        }

        body
    }
}

/// Encode canonical messages into the OpenAI wire format.
///
/// `request.system` becomes the first `system` message, matching what the Anthropic provider does
/// with its top-level `system` field. Tool calls and their results use the protocol's own fields,
/// so a local llama.cpp model sees the output of a command as a `tool` message rather than loose
/// prose.
fn encode_messages(messages: &[Message], system: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(prompt) = system
        && !prompt.is_empty()
    {
        out.push(json!({ "role": "system", "content": prompt }));
    }

    for message in messages {
        let mut text = Vec::new();
        let mut tool_calls = Vec::new();
        let mut results = Vec::new();
        for block in &message.blocks {
            match block {
                seek_core::Block::Text { text: part } => text.push(part.clone()),
                seek_core::Block::Thinking { text: part, .. } => text.push(part.clone()),
                seek_core::Block::ToolUse { id, name, input } => {
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(input).unwrap_or_default(),
                        },
                    }));
                }
                seek_core::Block::ToolResult {
                    id,
                    content,
                    is_error,
                } => {
                    let body = if *is_error {
                        format!("error: {content}")
                    } else {
                        content.clone()
                    };
                    results.push(json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "content": body,
                    }));
                }
            }
        }

        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        if !text.is_empty() || !tool_calls.is_empty() {
            let mut value = json!({ "role": role, "content": text.join("\n") });
            if !tool_calls.is_empty() {
                value["tool_calls"] = Value::Array(tool_calls);
            }
            out.push(value);
        }
        out.extend(results);
    }
    out
}

impl Provider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn stream<'a>(
        &'a self,
        request: ChatRequest,
    ) -> BoxStream<'a, Result<ProviderEvent, ProviderError>> {
        let body = self.build_body(&request);
        let url = self.url();
        let key = self.api_key.clone();
        let client = self.client.clone();

        async_stream::stream! {
            // The client side limit is honored before the request goes out (SPEC 6).
            let wait = {
                let mut limiter = self.limiter.lock().expect("the limiter is not poisoned");
                limiter.wait_time(0)
            };
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            {
                let mut limiter = self.limiter.lock().expect("the limiter is not poisoned");
                limiter.record(0);
            }

            let mut builder = client.post(&url).json(&body);
            if let Some(key) = key {
                builder = builder.header("authorization", format!("Bearer {key}"));
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(error) => {
                    // The cause is in the source of the error: without it the message is
                    // only "error sending request for url", which says nothing.
                    let mut cause = error.source();
                    let mut detail = String::new();
                    while let Some(source) = cause {
                        detail.push_str(&format!(": {source}"));
                        cause = source.source();
                    }
                    yield Err(ProviderError::Request(format!("{error}{detail}")));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                yield Err(ProviderError::Request(format!("{status}: {text}")));
                return;
            }

            // The provider may report its rate limit in the headers (SPEC 6).
            let limit = crate::ratelimit::from_headers(response.headers());
            if limit.is_reported() {
                yield Ok(ProviderEvent::RateLimit(limit));
            }

            let mut bytes = response.bytes_stream();
            let mut sse = crate::sse::SseDecoder::new();
            let mut decoder = OpenAiDecoder::default();

            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(ProviderError::Stream(error.to_string()));
                        return;
                    }
                };
                let text = String::from_utf8_lossy(&chunk).to_string();
                for event in sse.push(&text) {
                    if event.data.trim() == "[DONE]" {
                        for event in decoder.finish() {
                            yield Ok(event);
                        }
                        yield Ok(ProviderEvent::Done);
                        return;
                    }
                    match decoder.push(&event) {
                        Ok(events) => {
                            for event in events {
                                yield Ok(event);
                            }
                        }
                        Err(error) => {
                            yield Err(error);
                            return;
                        }
                    }
                }
            }
        }
        .boxed()
    }
}

/// A tool call that is still being streamed.
#[derive(Debug, Default, Clone)]
struct ToolCallState {
    id: String,
    name: String,
    arguments: String,
}

/// Turns OpenAI SSE events into provider events.
#[derive(Debug, Default)]
pub struct OpenAiDecoder {
    tool_calls: BTreeMap<u32, ToolCallState>,
    input_tokens: u32,
    output_tokens: u32,
}

impl OpenAiDecoder {
    pub fn new() -> Self {
        OpenAiDecoder::default()
    }

    /// Decode one event.
    pub fn push(&mut self, event: &SseEvent) -> Result<Vec<ProviderEvent>, ProviderError> {
        let data: Value = serde_json::from_str(&event.data)
            .map_err(|error| ProviderError::Stream(format!("bad json: {error}")))?;

        let mut out = Vec::new();

        if let Some(error) = data.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(ProviderError::Stream(message.to_string()));
        }

        if let Some(usage) = data.get("usage").filter(|usage| !usage.is_null()) {
            self.input_tokens = usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(self.input_tokens as u64) as u32;
            self.output_tokens = usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(self.output_tokens as u64) as u32;
            out.push(ProviderEvent::Usage {
                input: self.input_tokens,
                output: self.output_tokens,
            });
        }

        let Some(choices) = data.get("choices").and_then(Value::as_array) else {
            return Ok(out);
        };

        for choice in choices {
            let delta = choice.get("delta");
            if let Some(reasoning) = delta
                .and_then(|delta| delta.get("reasoning_content"))
                .and_then(Value::as_str)
                && !reasoning.is_empty()
            {
                out.push(ProviderEvent::ThinkingDelta(reasoning.to_string()));
            }
            if let Some(text) = delta
                .and_then(|delta| delta.get("content"))
                .and_then(Value::as_str)
                && !text.is_empty()
            {
                out.push(ProviderEvent::TextDelta(text.to_string()));
            }
            if let Some(calls) = delta
                .and_then(|delta| delta.get("tool_calls"))
                .and_then(Value::as_array)
            {
                for call in calls {
                    let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                    let state = self.tool_calls.entry(index).or_default();
                    if let Some(id) = call.get("id").and_then(Value::as_str) {
                        state.id = id.to_string();
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        state.name = name.to_string();
                    }
                    if let Some(arguments) =
                        call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        state.arguments.push_str(arguments);
                    }
                    // The call is shown while it is written: the arguments grow.
                    if !state.name.is_empty() {
                        out.push(ProviderEvent::ToolCallDelta {
                            id: state.id.clone(),
                            name: state.name.clone(),
                            arguments: state.arguments.clone(),
                        });
                    }
                }
            }
        }

        Ok(out)
    }

    /// Finish the stream: any tool call that was still being streamed is emitted now.
    pub fn finish(&mut self) -> Vec<ProviderEvent> {
        let mut out = Vec::new();
        let calls = std::mem::take(&mut self.tool_calls);
        for (index, state) in calls {
            let input: Value = if state.arguments.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&state.arguments).unwrap_or(json!({}))
            };
            out.push(ProviderEvent::ToolCall {
                id: if state.id.is_empty() {
                    format!("call_{index}")
                } else {
                    state.id
                },
                name: state.name,
                input,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_carries_stream_options() {
        let provider = OpenAiProvider::new("https://openrouter.ai/api/v1", None, "model", 4096);
        let body = provider.build_body(&ChatRequest::default());
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["stream_options"]["include_usage"], json!(true));
    }

    #[test]
    fn the_system_prompt_is_the_first_message() {
        let provider = OpenAiProvider::new("http://localhost/v1", None, "model", 4096);
        let request = ChatRequest {
            system: Some("You are SEEK.".into()),
            ..ChatRequest::default()
        };
        let body = provider.build_body(&request);
        assert_eq!(body["messages"][0]["role"], json!("system"));
        assert_eq!(body["messages"][0]["content"], json!("You are SEEK."));
    }

    #[test]
    fn tool_calls_and_results_use_the_wire_protocol() {
        use seek_core::{Block, Message, Role};
        let provider = OpenAiProvider::new("http://localhost/v1", None, "model", 4096);
        let request = ChatRequest {
            messages: vec![
                Message::new(
                    Role::Assistant,
                    vec![Block::ToolUse {
                        id: "c1".into(),
                        name: "bash".into(),
                        input: json!({"command": "echo hi"}),
                    }],
                ),
                Message::new(
                    Role::User,
                    vec![Block::ToolResult {
                        id: "c1".into(),
                        content: "hi".into(),
                        is_error: false,
                    }],
                ),
            ],
            ..ChatRequest::default()
        };
        let body = provider.build_body(&request);
        let messages = body["messages"].as_array().expect("an array");
        assert_eq!(messages[0]["role"], json!("assistant"));
        assert_eq!(messages[0]["tool_calls"][0]["id"], json!("c1"));
        assert_eq!(messages[1]["role"], json!("tool"));
        assert_eq!(messages[1]["tool_call_id"], json!("c1"));
        assert_eq!(messages[1]["content"], json!("hi"));
    }

    #[test]
    fn the_url_follows_the_base_url() {
        let provider = OpenAiProvider::new("http://127.0.0.1:8100/v1", None, "model", 4096);
        assert_eq!(provider.url(), "http://127.0.0.1:8100/v1/chat/completions");
    }

    #[test]
    fn extra_body_fields_are_merged() {
        let provider = OpenAiProvider::new("http://127.0.0.1:8100/v1", None, "model", 4096)
            .with_extra_body(json!({ "reasoning_effort": "medium" }));
        let body = provider.build_body(&ChatRequest::default());
        assert_eq!(body["reasoning_effort"], "medium");
    }

    #[test]
    fn a_tool_call_is_shown_while_it_is_written() {
        let mut decoder = OpenAiDecoder::new();
        // The name and the first part of the arguments.
        let event = SseEvent::data(
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "id": "c1",
                "function": { "name": "bash", "arguments": "{\"command\":\"ls" } }] } }] })
            .to_string(),
        );
        let events = decoder.push(&event).unwrap();
        let delta = events
            .iter()
            .find(|event| matches!(event, ProviderEvent::ToolCallDelta { .. }))
            .expect("the call is shown");
        match delta {
            ProviderEvent::ToolCallDelta {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "c1");
                assert_eq!(name, "bash");
                assert!(arguments.contains("ls"), "{arguments}");
            }
            _ => unreachable!(),
        }

        // The rest of the arguments.
        let event = SseEvent::data(
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0,
                "function": { "arguments": " -la\"}" } }] } }] })
            .to_string(),
        );
        let events = decoder.push(&event).unwrap();
        let delta = events
            .iter()
            .find(|event| matches!(event, ProviderEvent::ToolCallDelta { .. }))
            .expect("the call is shown");
        match delta {
            ProviderEvent::ToolCallDelta { arguments, .. } => {
                assert!(arguments.contains("ls -la"), "{arguments}");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn text_deltas_become_text_events() {
        let mut decoder = OpenAiDecoder::new();
        let event =
            SseEvent::data(json!({ "choices": [{ "delta": { "content": "hello" } }] }).to_string());
        let events = decoder.push(&event).unwrap();
        assert_eq!(events, vec![ProviderEvent::TextDelta("hello".into())]);
    }

    #[test]
    fn reasoning_content_becomes_a_thinking_event() {
        let mut decoder = OpenAiDecoder::new();
        let event = SseEvent::data(
            json!({ "choices": [{ "delta": { "reasoning_content": "thinking" } }] }).to_string(),
        );
        let events = decoder.push(&event).unwrap();
        assert_eq!(
            events,
            vec![ProviderEvent::ThinkingDelta("thinking".into())]
        );
    }

    #[test]
    fn tool_call_arguments_are_accumulated_by_index() {
        let mut decoder = OpenAiDecoder::new();
        let first = SseEvent::data(
            json!({
                "choices": [{ "delta": { "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "function": { "name": "read", "arguments": "{\"pa" }
                }] } }]
            })
            .to_string(),
        );
        decoder.push(&first).unwrap();
        let second = SseEvent::data(
            json!({
                "choices": [{ "delta": { "tool_calls": [{
                    "index": 0,
                    "function": { "arguments": "th\":\"a.txt\"}" }
                }] } }]
            })
            .to_string(),
        );
        decoder.push(&second).unwrap();

        let events = decoder.finish();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ProviderEvent::ToolCall { id, name, input } => {
                assert_eq!(id, "call_1");
                assert_eq!(name, "read");
                assert_eq!(input["path"], "a.txt");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn usage_is_read_from_the_final_chunk() {
        let mut decoder = OpenAiDecoder::new();
        let event = SseEvent::data(
            json!({
                "choices": [],
                "usage": { "prompt_tokens": 42, "completion_tokens": 7 }
            })
            .to_string(),
        );
        let events = decoder.push(&event).unwrap();
        assert_eq!(
            events,
            vec![ProviderEvent::Usage {
                input: 42,
                output: 7
            }]
        );
    }
}

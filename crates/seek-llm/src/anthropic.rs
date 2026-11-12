//! The Anthropic Messages provider (SPEC 6).
//!
//! `POST /v1/messages` with the headers `x-api-key` and `anthropic-version: 2023-06-01` and
//! `stream: true`. The events are `message_start`, `content_block_start`, `content_block_delta`,
//! `content_block_stop`, `message_delta` and `message_stop`.
//!
//! Tool input arrives as `input_json_delta` partial strings: they are accumulated per block index
//! and parsed when the block stops.

use std::collections::BTreeMap;

use futures::StreamExt;
use futures::stream::BoxStream;
use seek_core::{Message, Role};
use serde_json::{Value, json};

use crate::provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};
use crate::sse::SseEvent;

/// The Anthropic API version SEEK sends.
pub const API_VERSION: &str = "2023-06-01";

/// A provider that talks to the Anthropic Messages API.
pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
    /// The client side limits of the configuration (SPEC 6).
    limiter: std::sync::Mutex<crate::ratelimit::Limiter>,
}

impl AnthropicProvider {
    /// A provider for one model.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        max_tokens: u32,
    ) -> Self {
        AnthropicProvider {
            client: crate::client::a_client(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            max_tokens,
            limiter: std::sync::Mutex::new(crate::ratelimit::Limiter::new(
                crate::ratelimit::Limit::none(),
            )),
        }
    }

    /// The client side limits of the configuration (SPEC 6).
    pub fn with_limit(mut self, limit: crate::ratelimit::Limit) -> Self {
        self.limiter = std::sync::Mutex::new(crate::ratelimit::Limiter::new(limit));
        self
    }

    /// The URL of the messages endpoint.
    pub fn url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    /// Build the request body.
    ///
    /// The stable prefix — the system prompt and the beginning of the conversation — carries a
    /// `cache_control` breakpoint, so that Anthropic can cache it (SPEC 6).
    pub fn build_body(&self, request: &ChatRequest) -> Value {
        let (system, messages) = encode_messages(&request.messages);

        let mut body = json!({
            "model": self.model,
            "max_tokens": request.max_tokens.unwrap_or(self.max_tokens),
            "stream": true,
        });

        // The system prompt of the request wins over the system messages of the history.
        match &request.system {
            Some(prompt) => body["system"] = json!({ "type": "text", "text": prompt }),
            None => {
                if let Some(system) = system {
                    body["system"] = system;
                }
            }
        }
        body["messages"] = Value::Array(messages);

        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": serde_json::from_str::<Value>(&tool.parameters)
                            .unwrap_or(Value::Null),
                    })
                })
                .collect();
            body["tools"] = Value::Array(tools);
        }

        mark_cache_breakpoint(&mut body);
        body
    }
}

/// Encode the canonical messages into the Anthropic wire format.
fn encode_messages(messages: &[Message]) -> (Option<Value>, Vec<Value>) {
    let mut system: Option<Value> = None;
    let mut out = Vec::new();

    for message in messages {
        match message.role {
            Role::System => {
                let text = message.plain_text();
                system = Some(match system {
                    Some(existing) => {
                        // Keep both system messages in one block.
                        let old = existing.as_str().unwrap_or_default().to_string();
                        json!([{ "type": "text", "text": old }, { "type": "text", "text": text }])
                    }
                    None => json!({ "type": "text", "text": text }),
                });
            }
            Role::User | Role::Assistant => {
                let role = if message.role == Role::User {
                    "user"
                } else {
                    "assistant"
                };
                let mut content = Vec::new();
                for block in &message.blocks {
                    match block {
                        seek_core::Block::Text { text } => {
                            content.push(json!({ "type": "text", "text": text }));
                        }
                        seek_core::Block::Thinking {
                            signature, text, ..
                        } => {
                            let mut thinking = json!({ "type": "thinking", "thinking": text });
                            if let Some(signature) = signature {
                                thinking["signature"] = json!(signature);
                            }
                            content.push(thinking);
                        }
                        seek_core::Block::ToolUse { id, name, input } => {
                            content.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        seek_core::Block::ToolResult {
                            id,
                            content: result,
                            is_error,
                        } => {
                            content.push(json!({
                                "type": "tool_result",
                                "tool_use_id": id,
                                "content": result,
                                "is_error": is_error,
                            }));
                        }
                    }
                }
                if !content.is_empty() {
                    out.push(json!({ "role": role, "content": content }));
                }
            }
        }
    }

    (system, out)
}

/// Put a `cache_control` breakpoint on the last block of the stable prefix.
///
/// The stable prefix is the system prompt and the first user message; everything after it changes
/// from turn to turn and would break the cache (SPEC 9.2).
fn mark_cache_breakpoint(body: &mut Value) {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(first) = messages.first_mut() else {
        return;
    };
    let Some(content) = first.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(last) = content.last_mut() else {
        return;
    };
    last["cache_control"] = json!({ "type": "ephemeral" });
}

impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn protocol(&self) -> Protocol {
        Protocol::AnthropicMessages
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

            let response = match client
                .post(&url)
                .header("x-api-key", key)
                .header("anthropic-version", API_VERSION)
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Err(ProviderError::Request(error.to_string()));
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
            let mut decoder = AnthropicDecoder::default();

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

/// The state of one content block.
#[derive(Debug, Clone)]
enum BlockState {
    Text,
    Thinking,
    ToolUse {
        id: String,
        name: String,
        partial_json: String,
    },
}

/// Turns Anthropic SSE events into provider events.
#[derive(Debug, Default)]
pub struct AnthropicDecoder {
    blocks: BTreeMap<u32, BlockState>,
    input_tokens: u32,
    output_tokens: u32,
}

impl AnthropicDecoder {
    pub fn new() -> Self {
        AnthropicDecoder::default()
    }

    /// Decode one event.
    pub fn push(&mut self, event: &SseEvent) -> Result<Vec<ProviderEvent>, ProviderError> {
        let kind = event.event.clone().unwrap_or_else(|| "message".to_string());
        let data: Value = serde_json::from_str(&event.data)
            .map_err(|error| ProviderError::Stream(format!("bad json: {error}")))?;

        let mut out = Vec::new();
        match kind.as_str() {
            "message_start" => {
                let input = data
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                self.input_tokens = input;
                out.push(ProviderEvent::Usage { input, output: 0 });
            }
            "content_block_start" => {
                let index = data.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                let block = data.get("content_block");
                let state = match block.map(|block| block.get("type").and_then(Value::as_str)) {
                    Some(Some("tool_use")) => BlockState::ToolUse {
                        id: block
                            .and_then(|block| block.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        name: block
                            .and_then(|block| block.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        partial_json: String::new(),
                    },
                    Some(Some("thinking")) => BlockState::Thinking,
                    _ => BlockState::Text,
                };
                self.blocks.insert(index, state);
            }
            "content_block_delta" => {
                let index = data.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                let delta = data.get("delta");
                let kind = delta
                    .and_then(|delta| delta.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match kind {
                    "text_delta" => {
                        let text = delta
                            .and_then(|delta| delta.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !text.is_empty() {
                            out.push(ProviderEvent::TextDelta(text.to_string()));
                        }
                    }
                    "thinking_delta" => {
                        let text = delta
                            .and_then(|delta| delta.get("thinking"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !text.is_empty() {
                            out.push(ProviderEvent::ThinkingDelta(text.to_string()));
                        }
                    }
                    "input_json_delta" => {
                        let partial = delta
                            .and_then(|delta| delta.get("partial_json"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let mut delta_event = None;
                        if let Some(BlockState::ToolUse {
                            id,
                            name,
                            partial_json,
                        }) = self.blocks.get_mut(&index)
                        {
                            partial_json.push_str(partial);
                            if !name.is_empty() {
                                delta_event = Some(ProviderEvent::ToolCallDelta {
                                    id: id.clone(),
                                    name: name.clone(),
                                    arguments: partial_json.clone(),
                                });
                            }
                        }
                        // The call is shown while it is written.
                        if let Some(event) = delta_event {
                            out.push(event);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = data.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                if let Some(BlockState::ToolUse {
                    id,
                    name,
                    partial_json,
                }) = self.blocks.remove(&index)
                {
                    let input: Value = if partial_json.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&partial_json).map_err(|error| {
                            ProviderError::Stream(format!("bad tool input: {error}"))
                        })?
                    };
                    out.push(ProviderEvent::ToolCall { id, name, input });
                }
            }
            "message_delta" => {
                let output = data
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                self.output_tokens = output;
                out.push(ProviderEvent::Usage {
                    input: self.input_tokens,
                    output,
                });
            }
            "message_stop" => {
                out.push(ProviderEvent::Done);
            }
            "error" => {
                let message = data
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(ProviderError::Stream(message.to_string()));
            }
            _ => {}
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderEvent;

    #[test]
    fn the_system_prompt_becomes_the_system_field() {
        let provider = AnthropicProvider::new("https://api.anthropic.com", "key", "model", 4096);
        let request = ChatRequest {
            messages: vec![
                seek_core::Message::new(Role::System, vec![seek_core::Block::text("be brief")]),
                seek_core::Message::user("hello"),
            ],
            ..ChatRequest::default()
        };
        let body = provider.build_body(&request);
        assert_eq!(body["system"]["text"], "be brief");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
        assert_eq!(body["stream"], json!(true));
    }

    #[test]
    fn the_stable_prefix_carries_a_cache_breakpoint() {
        let provider = AnthropicProvider::new("https://api.anthropic.com", "key", "model", 4096);
        let request = ChatRequest {
            messages: vec![
                seek_core::Message::user("first"),
                seek_core::Message::user("second"),
            ],
            ..ChatRequest::default()
        };
        let body = provider.build_body(&request);
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );
        assert!(
            body["messages"][1]["content"][0]
                .get("cache_control")
                .is_none()
        );
    }

    #[test]
    fn tools_are_sent_with_their_schema() {
        let provider = AnthropicProvider::new("https://api.anthropic.com", "key", "model", 4096);
        let request = ChatRequest {
            tools: vec![seek_core::DEFAULT_TOOLS[0].clone()],
            ..ChatRequest::default()
        };
        let body = provider.build_body(&request);
        assert_eq!(body["tools"][0]["name"], "read");
        assert!(body["tools"][0]["input_schema"]["properties"]["path"].is_object());
    }

    #[test]
    fn tool_input_is_accumulated_and_parsed_at_the_end_of_the_block() {
        let mut decoder = AnthropicDecoder::new();
        let start = SseEvent {
            event: Some("content_block_start".into()),
            data: json!({
                "index": 0,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "read" }
            })
            .to_string(),
        };
        assert!(decoder.push(&start).unwrap().is_empty());

        for part in ["{\"pa", "th\": \"a.txt\"}"] {
            let delta = SseEvent {
                event: Some("content_block_delta".into()),
                data: json!({
                    "index": 0,
                    "delta": { "type": "input_json_delta", "partial_json": part }
                })
                .to_string(),
            };
            // The call is shown while it is written: the arguments grow.
            let events = decoder.push(&delta).unwrap();
            let delta_event = events
                .iter()
                .find(|event| matches!(event, ProviderEvent::ToolCallDelta { .. }))
                .expect("the call is shown");
            match delta_event {
                ProviderEvent::ToolCallDelta {
                    id,
                    name,
                    arguments,
                } => {
                    assert_eq!(id, "toolu_1");
                    assert_eq!(name, "read");
                    assert!(arguments.contains("pa"), "{arguments}");
                }
                _ => unreachable!(),
            }
        }

        let stop = SseEvent {
            event: Some("content_block_stop".into()),
            data: json!({ "index": 0 }).to_string(),
        };
        let events = decoder.push(&stop).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ProviderEvent::ToolCall { id, name, input } => {
                assert_eq!(id, "toolu_1");
                assert_eq!(name, "read");
                assert_eq!(input["path"], "a.txt");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn usage_is_reported_twice() {
        let mut decoder = AnthropicDecoder::new();
        let start = SseEvent {
            event: Some("message_start".into()),
            data: json!({ "message": { "usage": { "input_tokens": 120 } } }).to_string(),
        };
        let events = decoder.push(&start).unwrap();
        assert_eq!(
            events,
            vec![ProviderEvent::Usage {
                input: 120,
                output: 0
            }]
        );

        let delta = SseEvent {
            event: Some("message_delta".into()),
            data:
                json!({ "delta": { "stop_reason": "end_turn" }, "usage": { "output_tokens": 30 } })
                    .to_string(),
        };
        let events = decoder.push(&delta).unwrap();
        assert_eq!(
            events,
            vec![ProviderEvent::Usage {
                input: 120,
                output: 30
            }]
        );
    }

    #[test]
    fn an_error_event_is_an_error() {
        let mut decoder = AnthropicDecoder::new();
        let event = SseEvent {
            event: Some("error".into()),
            data: json!({ "error": { "message": "overloaded" } }).to_string(),
        };
        let error = decoder.push(&event).unwrap_err();
        assert!(error.to_string().contains("overloaded"));
    }
}

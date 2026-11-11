//! A scripted provider.
//!
//! Milestone v0.1 drives the Chat layout with a fake agent (SPEC 15) — no network, no API key,
//! no model. The script is a list of events that are emitted with a small delay so the UI shows
//! the same streaming behaviour it will show against a real provider.

use futures::StreamExt;
use futures::stream::BoxStream;
use seek_core::Message;

use crate::provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};

/// A provider that replays a fixed script.
#[derive(Debug, Clone)]
pub struct FakeProvider {
    name: String,
    model: String,
    script: Vec<ProviderEvent>,
    /// Delay between two events, in milliseconds.
    step_ms: u64,
}

impl FakeProvider {
    /// A provider that replays `script`.
    pub fn scripted(
        name: impl Into<String>,
        model: impl Into<String>,
        script: Vec<ProviderEvent>,
    ) -> Self {
        FakeProvider {
            name: name.into(),
            model: model.into(),
            script,
            step_ms: 40,
        }
    }

    /// The demo turn used by the v0.1 UI: a bit of reasoning, then an answer.
    pub fn demo() -> Self {
        FakeProvider::scripted(
            "fake",
            "fake/demo",
            vec![
                ProviderEvent::ThinkingDelta("The user wants to see the layout".into()),
                ProviderEvent::ThinkingDelta(" working.".into()),
                ProviderEvent::TextDelta("Hello from SEEK.".into()),
                ProviderEvent::TextDelta(" This turn is driven by a fake provider.".into()),
                ProviderEvent::Usage {
                    input: 412,
                    output: 18,
                },
                ProviderEvent::Done,
            ],
        )
    }

    /// Replace the delay between two events.
    pub fn with_step(mut self, step_ms: u64) -> Self {
        self.step_ms = step_ms;
        self
    }
}

impl Provider for FakeProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn stream<'a>(
        &'a self,
        _request: ChatRequest,
    ) -> BoxStream<'a, Result<ProviderEvent, ProviderError>> {
        let script = self.script.clone();
        let step = std::time::Duration::from_millis(self.step_ms);
        futures::stream::iter(script.into_iter().map(Ok))
            .then(move |event| async move {
                tokio::time::sleep(step).await;
                event
            })
            .boxed()
    }
}

/// Build a request from a plain conversation.
pub fn request(messages: Vec<Message>) -> ChatRequest {
    ChatRequest {
        messages,
        ..ChatRequest::default()
    }
}

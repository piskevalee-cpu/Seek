//! SEEK providers: wire protocols, streaming, the model catalog and rate limiting (SPEC 4.1).
//!
//! Milestone v0.1 only needs the provider boundary and a scripted provider so that the TUI can be
//! driven without a network. The Anthropic Messages and OpenAI-compatible implementations land in
//! milestone v0.2.

pub mod anthropic;
pub mod catalog;
pub mod fake;
pub mod openai;
pub mod provider;
pub mod sse;

pub use anthropic::AnthropicProvider;
pub use catalog::{ModelInfo, Variant};
pub use fake::FakeProvider;
pub use openai::OpenAiProvider;
pub use provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};
pub use sse::{SseDecoder, SseEvent};

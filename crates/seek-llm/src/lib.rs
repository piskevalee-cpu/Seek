//! SEEK providers: wire protocols, streaming, the model catalog and rate limiting (SPEC 4.1).
//!
//! Milestone v0.1 only needs the provider boundary and a scripted provider so that the TUI can be
//! driven without a network. The Anthropic Messages and OpenAI-compatible implementations land in
//! milestone v0.2.

pub mod fake;
pub mod provider;

pub use fake::FakeProvider;
pub use provider::{ChatRequest, Protocol, Provider, ProviderError, ProviderEvent};

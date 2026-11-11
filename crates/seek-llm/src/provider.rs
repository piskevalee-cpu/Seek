//! The provider boundary.
//!
//! The interface itself lives in `seek-core`, because the agent loop lives there. This module
//! re-exports it, so `seek_llm::Provider` keeps working.

pub use seek_core::provider::{
    BoxProviderStream, ChatRequest, Protocol, Provider, ProviderError, ProviderEvent,
};

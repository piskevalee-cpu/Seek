//! SEEK local models: the llama.cpp supervisor (SPEC 4.1, 7).
//!
//! This crate owns the lifecycle of the local server: scanning `~/seek/models/`, picking a port,
//! spawning `llama serve`, waiting for health, reading the real context size and stopping it
//! again. It also owns tokenization against the running server.
//!
//! Milestone v0.1 does not need any of that yet — the TUI runs against the fake provider — so the
//! crate is empty for now. The supervisor lands in milestone v0.4.

/// The folder where local models live, relative to the SEEK home.
pub const MODELS_DIR: &str = "models";

/// The first port used for local servers; the port for a model is derived from its id
/// (SPEC 7.2).
pub const PORT_RANGE_START: u16 = 8100;

/// The last port used for local servers.
pub const PORT_RANGE_END: u16 = 8899;

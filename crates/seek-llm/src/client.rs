//! The HTTP client of the providers.
//!
//! The llama.cpp server closes the connection after every answer, although it says it
//! keeps it alive. A client that reuses a connection from its pool then gets "connection
//! closed before message completed" on the next request. The pool is therefore empty: every
//! request opens its own connection, which costs nothing on a local server.

use reqwest::Client;

/// A client that opens a connection for every request.
pub fn a_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap_or_default()
}

//! The port of a local model (SPEC 7.2).
//!
//! The port is derived from the model id, so a model always gets the same port. It is confirmed
//! with a bind test and then persisted in the sidecar, so one port per model.

use std::net::TcpListener;

/// The first port of the range.
pub const PORT_START: u16 = 8100;
/// The last port of the range.
pub const PORT_END: u16 = 8899;

/// The port of a model, derived from its id.
///
/// The same id always gives the same port.
pub fn port_for(model_id: &str) -> u16 {
    let hash = fnv1a(model_id.as_bytes());
    let span = (PORT_END - PORT_START + 1) as u64;
    PORT_START + (hash % span) as u16
}

/// A 64-bit FNV-1a hash.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Whether a port can be bound right now.
pub fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find a free port for a model.
///
/// The port of the model is tried first; if it is taken, the next ones in the range are tried.
pub fn find_free_port(model_id: &str) -> Option<u16> {
    let start = port_for(model_id);
    let span = PORT_END - PORT_START + 1;
    for offset in 0..span {
        let port = PORT_START + ((start - PORT_START + offset) % span);
        if is_free(port) {
            return Some(port);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_is_the_same_for_the_same_model() {
        assert_eq!(port_for("local/mistral-7b"), port_for("local/mistral-7b"));
    }

    #[test]
    fn the_port_is_in_the_range() {
        for name in ["local/a", "local/b", "local/mistral-7b-q4", "local/x"] {
            let port = port_for(name);
            assert!((PORT_START..=PORT_END).contains(&port));
        }
    }

    #[test]
    fn different_models_get_different_ports() {
        // Not a hash collision, but the ids must not all map to the same port.
        let ports: std::collections::HashSet<u16> = (0..20)
            .map(|n| port_for(&format!("local/model-{n}")))
            .collect();
        assert!(ports.len() > 10);
    }

    #[test]
    fn a_free_port_is_found() {
        let port = find_free_port("local/test").expect("a port is found");
        assert!((PORT_START..=PORT_END).contains(&port));
    }
}

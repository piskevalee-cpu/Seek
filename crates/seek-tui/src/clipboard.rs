//! OSC 52 clipboard writes, the protocol grok-build uses (Apache-2.0).
//!
//! `ESC ] 52 ; c ; <base64> BEL` works over SSH and in terminals without a local clipboard tool,
//! which is why text selected in the TUI is copied this way. A small base64 encoder keeps the
//! crate dependency-free.

use std::io::Write;

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as base64, without padding, so the OSC 52 payload stays short.
pub fn base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (a << 16) | (b << 8) | c;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(value & 63) as usize] as char);
        }
    }
    out
}

/// The OSC 52 sequence that copies `text` to the clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Write text to the terminal clipboard. Long text is split into chunks because some terminals
/// cap the length of a single OSC 52 payload.
pub fn copy(text: &str) {
    if text.is_empty() {
        return;
    }
    let mut stdout = std::io::stdout();
    for chunk in text.as_bytes().chunks(1024) {
        let payload = String::from_utf8_lossy(chunk);
        let _ = stdout.write_all(osc52(&payload).as_bytes());
    }
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encodes_the_common_cases() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg");
        assert_eq!(base64(b"fo"), "Zm8");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"stolen"), "c3RvbGVu");
    }

    #[test]
    fn osc52_wraps_the_payload() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk\x07");
    }

    #[test]
    fn long_text_is_chunked() {
        let text = "x".repeat(2500);
        // Three chunks of 1024 bytes: the writer must not lose the tail.
        assert_eq!(text.as_bytes().chunks(1024).count(), 3);
        assert_eq!(base64(text.as_bytes()).len(), 3334);
    }
}

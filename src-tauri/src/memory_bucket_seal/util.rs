// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared utilities for the bucket-seal module.
//!
//! `redact` returns a short, non-revealing token for log lines that would
//! otherwise leak source ids (email addresses, channel names). Used by
//! both `paths.rs` and `chunker.rs` when logging fallback paths.

use sha2::{Digest, Sha256};

/// Redact a string to a short non-revealing token for log lines.
///
/// Returns the first 8 hex characters of `SHA-256(input)`. Sufficient for
/// deduplicated logging of malformed source ids without leaking PII.
pub fn redact(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    let hex_str = hex::encode(digest);
    hex_str[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_is_deterministic_for_same_input() {
        assert_eq!(redact("alice@example.com"), redact("alice@example.com"));
    }

    #[test]
    fn redact_differs_for_different_input() {
        assert_ne!(redact("alice@example.com"), redact("bob@example.com"));
    }

    #[test]
    fn redact_is_8_chars() {
        assert_eq!(redact("anything").len(), 8);
    }
}

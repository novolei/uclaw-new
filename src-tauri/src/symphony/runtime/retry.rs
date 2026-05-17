//! Symphony retry backoff — verbatim from the OpenAI Symphony SPEC:
//!
//! ```text
//! delay = min(10_000 * 2^(attempt - 1), max_retry_backoff_ms)
//! ```
//!
//! Default `max_retry_backoff_ms = 300_000` (5 min). `attempt` starts at 1
//! (representing "the **next** attempt about to begin", i.e. the first retry
//! is `attempt = 2`).

/// Compute backoff for the *next* attempt.
///
/// - `attempt = 1` → 10_000 ms (first retry happens after this much delay)
/// - `attempt = 2` → 20_000 ms
/// - `attempt = 3` → 40_000 ms
/// - … capped at `max_ms`.
///
/// `attempt = 0` is treated as `1` to avoid the underflow in `2^(0-1)`.
pub fn backoff_ms(attempt: u32, max_ms: u64) -> u64 {
    let a = attempt.max(1) as u64;
    // 2^(a-1), saturating to avoid overflow for absurd attempt counts.
    let factor = 1u64.checked_shl((a - 1) as u32).unwrap_or(u64::MAX);
    let raw = 10_000u64.saturating_mul(factor);
    raw.min(max_ms)
}

/// Whether to retry given the current attempt vs. the configured maximum.
/// `max_attempts = 1` means "no retry"; the first attempt is the only attempt.
pub fn should_retry(attempt: u32, max_attempts: u32) -> bool {
    attempt < max_attempts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_symphony_spec_progression() {
        assert_eq!(backoff_ms(1, 300_000), 10_000);
        assert_eq!(backoff_ms(2, 300_000), 20_000);
        assert_eq!(backoff_ms(3, 300_000), 40_000);
        assert_eq!(backoff_ms(4, 300_000), 80_000);
        assert_eq!(backoff_ms(5, 300_000), 160_000);
    }

    #[test]
    fn caps_at_max_ms() {
        assert_eq!(backoff_ms(10, 300_000), 300_000);
        assert_eq!(backoff_ms(100, 300_000), 300_000);
    }

    #[test]
    fn handles_zero_attempt_as_one() {
        assert_eq!(backoff_ms(0, 300_000), 10_000);
    }

    #[test]
    fn handles_absurd_attempt_without_panicking() {
        // 2^64 would overflow; saturating math + min cap keeps us sane.
        assert_eq!(backoff_ms(u32::MAX, 300_000), 300_000);
    }

    #[test]
    fn should_retry_semantics() {
        // max_attempts=1 means no retry.
        assert!(!should_retry(1, 1));
        // First retry: attempt=1 < max=3.
        assert!(should_retry(1, 3));
        // Last attempt.
        assert!(should_retry(2, 3));
        // Exhausted.
        assert!(!should_retry(3, 3));
        assert!(!should_retry(4, 3));
    }
}

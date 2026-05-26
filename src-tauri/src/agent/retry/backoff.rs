//! Pure exponential-backoff math for the agent retry loop.
//!
//! Sequence: 1s, 2s, 4s, 8s, 15s, 15s, 15s… (cap = `RETRY_MAX_DELAY_MS`).
//! Each output is multiplied by a ±20% jitter factor, then clamped to the
//! caller's `remaining_budget` so the cumulative sleep never exceeds it.

use rand::Rng;
use std::time::Duration;

pub const BASE_DELAY_MS: u64 = 1_000;
pub const RETRY_MAX_DELAY_MS: u64 = 15_000;
pub const JITTER_RATIO: f64 = 0.2;

/// Compute the next sleep duration.
///
/// `attempt` is 1-based — the first retry uses `attempt = 1` (1s base).
/// `remaining_budget` clamps the result; if the caller has no budget left,
/// returns `Duration::ZERO` so the caller treats it as "exhausted".
pub fn compute_delay<R: Rng>(attempt: u32, remaining_budget: Duration, rng: &mut R) -> Duration {
    if remaining_budget.is_zero() {
        return Duration::ZERO;
    }
    let exponent = attempt.saturating_sub(1).min(30);
    let raw = BASE_DELAY_MS.saturating_mul(2u64.saturating_pow(exponent));
    let capped = raw.min(RETRY_MAX_DELAY_MS);
    let jitter_factor = 1.0 + rng.gen_range(-JITTER_RATIO..=JITTER_RATIO);
    let jittered_ms = (capped as f64 * jitter_factor).max(0.0).round() as u64;
    let candidate = Duration::from_millis(jittered_ms);
    candidate.min(remaining_budget)
}

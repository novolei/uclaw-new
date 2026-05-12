//! W2: agent retry-budget extension.
//!
//! See `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4.

pub mod backoff;
pub mod budget;

#[cfg(test)]
mod tests;

pub use backoff::{BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS, compute_delay};
pub use budget::{BudgetDecision, MAX_AUTO_RETRIES, MAX_AUTO_RETRY_WAIT_MS, RetryBudget};

use serde::Serialize;

/// Event emitted to the frontend on every retry, plus when the budget is exhausted.
/// Channel name: `"agent:retry"`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum AgentRetryEvent {
    /// About to sleep, then retry.
    Starting {
        attempt: u32,
        max_attempts: u32,
        delay_seconds: f64,
        reason: String,
    },
    /// Just woke up; the retry is being made now.
    Attempt {
        attempt: u32,
        timestamp_ms: i64,
        reason: String,
    },
    /// Budget exhausted; no further retry will be attempted.
    Exhausted {
        total_attempts: u32,
        total_wait_ms: u64,
    },
}

impl AgentRetryEvent {
    pub const CHANNEL: &'static str = "agent:retry";
}

//! W2: agent retry-budget extension.
//!
//! See `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4.

pub mod backoff;
pub mod budget;

#[cfg(test)]
mod tests;

pub use backoff::{BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS, compute_delay};
pub use budget::{BudgetDecision, MAX_AUTO_RETRIES, MAX_AUTO_RETRY_WAIT_MS, RetryBudget};

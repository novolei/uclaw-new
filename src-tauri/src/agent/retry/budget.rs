//! Stateful retry budget for the agent loop.
//!
//! Tracks attempt count + cumulative sleep time. `next_delay()` is the only
//! mutator and returns `BudgetDecision::Sleep(d)` until the budget is gone,
//! then `BudgetDecision::Exhausted` permanently.

use super::backoff::compute_delay;
use rand::{thread_rng, RngCore};
use std::time::Duration;

pub const MAX_AUTO_RETRIES: u32 = 25;
pub const MAX_AUTO_RETRY_WAIT_MS: u64 = 5 * 60_000; // 5 minutes

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    /// Caller should sleep this long, then retry.
    Sleep(Duration),
    /// Caller should give up. No more retries this loop iteration.
    Exhausted,
}

/// Per-iteration retry budget. Cheap to construct. NOT Send/Sync-safe
/// across awaits — owned by a single loop frame.
#[derive(Debug)]
pub struct RetryBudget {
    max_attempts: u32,
    max_total_wait: Duration,
    elapsed_wait: Duration,
    attempts: u32,
}

impl RetryBudget {
    /// 25 attempts / 5 min total. Matches Proma v0.9.27 PR #419.
    pub fn for_agent_loop() -> Self {
        Self {
            max_attempts: MAX_AUTO_RETRIES,
            max_total_wait: Duration::from_millis(MAX_AUTO_RETRY_WAIT_MS),
            elapsed_wait: Duration::ZERO,
            attempts: 0,
        }
    }

    /// Construct with custom limits (tests).
    #[cfg(test)]
    pub fn with_limits(max_attempts: u32, max_total_wait_ms: u64) -> Self {
        Self {
            max_attempts,
            max_total_wait: Duration::from_millis(max_total_wait_ms),
            elapsed_wait: Duration::ZERO,
            attempts: 0,
        }
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
    pub fn elapsed_wait(&self) -> Duration {
        self.elapsed_wait
    }
    pub fn max_total_wait(&self) -> Duration {
        self.max_total_wait
    }

    /// Advance the budget by one attempt. Returns the requested sleep duration,
    /// or `Exhausted` when out of attempts or out of time.
    pub fn next_delay(&mut self) -> BudgetDecision {
        self.next_delay_with(&mut thread_rng())
    }

    /// Same as `next_delay` but uses a caller-supplied RNG (testability).
    pub fn next_delay_with<R: RngCore>(&mut self, rng: &mut R) -> BudgetDecision {
        if self.attempts >= self.max_attempts {
            return BudgetDecision::Exhausted;
        }
        let remaining = self.max_total_wait.saturating_sub(self.elapsed_wait);
        if remaining.is_zero() {
            return BudgetDecision::Exhausted;
        }
        self.attempts += 1;
        let delay = compute_delay(self.attempts, remaining, rng);
        if delay.is_zero() {
            return BudgetDecision::Exhausted;
        }
        self.elapsed_wait += delay;
        BudgetDecision::Sleep(delay)
    }
}

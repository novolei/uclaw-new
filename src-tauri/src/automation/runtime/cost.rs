//! Cost guardrails for automation runs. Two hard caps (distinct from the
//! observational cost_records / V13): a per-run cap that terminates a run
//! mid-loop, and a per-day cap checked before a run starts.

use std::sync::atomic::{AtomicU64, Ordering};

/// Resolved cost caps for one run, sourced from MemubotConfig.automation.
#[derive(Debug, Clone, Copy)]
pub struct CostCapConfig {
    pub per_run_usd: f64,
    pub per_day_usd: f64,
}

/// Mutable per-run cost accumulator. Stores hundred-thousandths of a USD
/// (USD * 100_000) in an AtomicU64 so the delegate can accumulate cost
/// across loop iterations without a Mutex<f64>.
#[derive(Debug)]
pub struct CostCapState {
    accumulated_micro: AtomicU64,
    per_run_micro: u64,
}

const MICRO_PER_USD: f64 = 100_000.0;

impl CostCapState {
    pub fn new(cap: CostCapConfig) -> Self {
        Self {
            accumulated_micro: AtomicU64::new(0),
            per_run_micro: (cap.per_run_usd * MICRO_PER_USD) as u64,
        }
    }

    /// Add `cost_usd` to the running total. Returns the new total in USD.
    pub fn add(&self, cost_usd: f64) -> f64 {
        let delta = (cost_usd.max(0.0) * MICRO_PER_USD) as u64;
        let prev = self.accumulated_micro.fetch_add(delta, Ordering::Relaxed);
        (prev + delta) as f64 / MICRO_PER_USD
    }

    /// Current accumulated cost in USD.
    pub fn total_usd(&self) -> f64 {
        self.accumulated_micro.load(Ordering::Relaxed) as f64 / MICRO_PER_USD
    }

    /// True once the per-run cap has been reached or exceeded.
    pub fn per_run_exceeded(&self) -> bool {
        self.accumulated_micro.load(Ordering::Relaxed) >= self.per_run_micro
    }
}

/// Result of a pre-run per-day cap check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostCapDecision {
    /// Under the per-day cap — the run may start.
    Allow,
    /// Day total is at/over the per-day cap — do not start the run.
    DenyPerDay,
}

/// Decide whether a run may start given the day's spend so far.
pub fn check_per_day(day_total_usd: f64, cap: CostCapConfig) -> CostCapDecision {
    if day_total_usd >= cap.per_day_usd {
        CostCapDecision::DenyPerDay
    } else {
        CostCapDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap() -> CostCapConfig {
        CostCapConfig { per_run_usd: 1.00, per_day_usd: 10.00 }
    }

    #[test]
    fn per_run_accumulates_and_trips_at_cap() {
        let state = CostCapState::new(cap());
        assert!(!state.per_run_exceeded());
        state.add(0.40);
        assert!(!state.per_run_exceeded());
        state.add(0.65); // total 1.05 >= 1.00
        assert!(state.per_run_exceeded());
        assert!((state.total_usd() - 1.05).abs() < 1e-6);
    }

    #[test]
    fn per_run_ignores_negative_cost() {
        let state = CostCapState::new(cap());
        state.add(-5.0);
        assert_eq!(state.total_usd(), 0.0);
    }

    #[test]
    fn per_day_allows_under_cap() {
        assert_eq!(check_per_day(9.99, cap()), CostCapDecision::Allow);
    }

    #[test]
    fn per_day_denies_at_or_over_cap() {
        assert_eq!(check_per_day(10.00, cap()), CostCapDecision::DenyPerDay);
        assert_eq!(check_per_day(12.50, cap()), CostCapDecision::DenyPerDay);
    }
}

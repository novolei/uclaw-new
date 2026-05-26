//! W2 retry-budget unit tests.

use super::backoff::{compute_delay, BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS};
use super::budget::{BudgetDecision, RetryBudget, MAX_AUTO_RETRIES, MAX_AUTO_RETRY_WAIT_MS};
use std::time::Duration;

// A deterministic RNG that always returns the same u64. Used to pin jitter
// behavior. `gen_range(-0.2..=0.2)` reads `next_u64()` and maps it onto the
// range. At u64::MAX/2 the jitter factor is ~0 (we tolerate ±20ms slop).
struct FixedRng(u64);
impl rand::RngCore for FixedRng {
    fn next_u32(&mut self) -> u32 {
        self.0 as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0
    }
    fn fill_bytes(&mut self, dst: &mut [u8]) {
        for b in dst.iter_mut() {
            *b = self.0 as u8;
        }
    }
    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dst);
        Ok(())
    }
}

fn rng_zero_jitter() -> FixedRng {
    FixedRng(u64::MAX / 2)
}
fn rng_min_jitter() -> FixedRng {
    FixedRng(0)
}
fn rng_max_jitter() -> FixedRng {
    FixedRng(u64::MAX)
}

#[test]
fn compute_delay_base_sequence_no_jitter() {
    let huge_budget = Duration::from_secs(3600);
    let d1 = compute_delay(1, huge_budget, &mut rng_zero_jitter());
    let d2 = compute_delay(2, huge_budget, &mut rng_zero_jitter());
    let d3 = compute_delay(3, huge_budget, &mut rng_zero_jitter());
    let d4 = compute_delay(4, huge_budget, &mut rng_zero_jitter());
    let d5 = compute_delay(5, huge_budget, &mut rng_zero_jitter());
    let d6 = compute_delay(6, huge_budget, &mut rng_zero_jitter());

    // With ~0 jitter the sequence is 1s, 2s, 4s, 8s, 15s, 15s
    // Slop of 100ms because FixedRng(u64::MAX/2) is not exactly the midpoint
    // for gen_range — its mapping involves arithmetic that can drift slightly.
    assert!(
        (d1.as_millis() as i64 - 1_000).abs() < 100,
        "attempt 1: {:?}",
        d1
    );
    assert!(
        (d2.as_millis() as i64 - 2_000).abs() < 100,
        "attempt 2: {:?}",
        d2
    );
    assert!(
        (d3.as_millis() as i64 - 4_000).abs() < 200,
        "attempt 3: {:?}",
        d3
    );
    assert!(
        (d4.as_millis() as i64 - 8_000).abs() < 400,
        "attempt 4: {:?}",
        d4
    );
    assert!(
        (d5.as_millis() as i64 - 15_000).abs() < 800,
        "attempt 5: {:?}",
        d5
    );
    assert!(
        (d6.as_millis() as i64 - 15_000).abs() < 800,
        "attempt 6 (capped): {:?}",
        d6
    );
}

#[test]
fn compute_delay_min_jitter_floors_at_minus_20_percent() {
    let d = compute_delay(5, Duration::from_secs(3600), &mut rng_min_jitter());
    // 15s * 0.8 = 12s
    assert!(
        d.as_millis() >= 11_900 && d.as_millis() <= 12_100,
        "got {:?}",
        d
    );
}

#[test]
fn compute_delay_max_jitter_ceils_at_plus_20_percent() {
    let d = compute_delay(5, Duration::from_secs(3600), &mut rng_max_jitter());
    // 15s * 1.2 = 18s — but clamped to remaining budget (3600s here, so unaffected)
    assert!(
        d.as_millis() >= 17_900 && d.as_millis() <= 18_100,
        "got {:?}",
        d
    );
}

#[test]
fn compute_delay_clamps_to_remaining_budget() {
    let tight = Duration::from_millis(500);
    let d = compute_delay(5, tight, &mut rng_max_jitter());
    assert_eq!(d, tight, "should clamp to remaining budget");
}

#[test]
fn compute_delay_zero_budget_returns_zero() {
    let d = compute_delay(1, Duration::ZERO, &mut rng_zero_jitter());
    assert_eq!(d, Duration::ZERO);
}

#[test]
fn budget_returns_sleep_until_attempts_exhausted() {
    let mut b = RetryBudget::with_limits(3, MAX_AUTO_RETRY_WAIT_MS);
    let mut rng = rng_zero_jitter();
    assert!(matches!(
        b.next_delay_with(&mut rng),
        BudgetDecision::Sleep(_)
    ));
    assert!(matches!(
        b.next_delay_with(&mut rng),
        BudgetDecision::Sleep(_)
    ));
    assert!(matches!(
        b.next_delay_with(&mut rng),
        BudgetDecision::Sleep(_)
    ));
    assert_eq!(b.next_delay_with(&mut rng), BudgetDecision::Exhausted);
    assert_eq!(b.attempts(), 3);
}

#[test]
fn budget_returns_exhausted_when_time_runs_out() {
    // 2000ms budget. With zero jitter: attempt 1 ≈ 1000ms, attempt 2 clamped to ~1000ms remaining.
    // Third call returns Exhausted because elapsed ≈ 2000ms.
    let mut b = RetryBudget::with_limits(99, 2_000);
    let mut rng = rng_zero_jitter();
    let d1 = b.next_delay_with(&mut rng);
    let d2 = b.next_delay_with(&mut rng);
    let d3 = b.next_delay_with(&mut rng);
    assert!(matches!(d1, BudgetDecision::Sleep(_)));
    assert!(matches!(d2, BudgetDecision::Sleep(_)));
    assert_eq!(
        d3,
        BudgetDecision::Exhausted,
        "third call should be exhausted, got {:?}",
        d3
    );
    assert!(b.elapsed_wait() <= Duration::from_millis(2_000));
}

#[test]
fn budget_default_for_agent_loop_uses_proma_constants() {
    let b = RetryBudget::for_agent_loop();
    assert_eq!(b.max_attempts(), MAX_AUTO_RETRIES);
    assert_eq!(b.max_attempts(), 25);
    assert_eq!(
        b.max_total_wait(),
        Duration::from_millis(MAX_AUTO_RETRY_WAIT_MS)
    );
    assert_eq!(b.max_total_wait(), Duration::from_secs(300));
    assert_eq!(b.elapsed_wait(), Duration::ZERO);
    assert_eq!(b.attempts(), 0);
}

// Suppress unused-import warnings if any constant ends up unread
#[allow(dead_code)]
const _SANITY: (u64, u64, f64) = (BASE_DELAY_MS, RETRY_MAX_DELAY_MS, JITTER_RATIO);

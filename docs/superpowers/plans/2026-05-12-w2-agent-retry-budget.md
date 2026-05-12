# W2 — Agent Retry Budget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the agent's stream-retry policy from 2 attempts / fixed backoff to **25 attempts / 5-minute cumulative budget / 15s-cap exponential backoff with ±20% jitter**, matching Proma v0.9.27 PR #419, and emit `agent:retry` IPC events so the UI can show retry progress in W4.

**Architecture:** New self-contained `src-tauri/src/agent/retry/` module (3 files + tests). A `RetryBudget` struct owns the attempt counter, the cumulative-sleep counter, and returns either `Some(delay)` or `None` (exhausted). A pure `backoff::compute_delay(attempt, jitter_source)` function computes the duration. Two existing retry sites in `agent/dispatcher.rs` (mid-stream + stream-setup, both currently using a private 2-retry / 500ms-base loop) swap their hard-coded constants for the shared budget.

**Tech Stack:** Rust · `tokio::time::sleep` for the wait · `tokio::select!` for abort-on-stop · `tauri::Emitter` for the new IPC event · standard `rand` crate (already in uClaw transitive deps; verify in Task 1) for jitter.

**Spec:** `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4 — note §4.2 (backup tolerance) is **YAGNI'd** because uClaw has no comparable backup/export feature (confirmed via grep).

---

## Pre-flight

- [ ] **Branch setup**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main
git pull --ff-only
git checkout -b claude/w2-agent-retry-budget
```

Expected: clean checkout at main's tip; new branch.

If `claude/w1-renderer-quick-wins` is merged before this branch is created, that's fine — main will already contain W1's docs (CLAUDE.md dual-composer rule, spec, plan). W2 doesn't depend on W1 code.

- [ ] **Baseline verification**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | tail -8
```

Record the existing test count (baseline). Confirm zero errors.

- [ ] **Confirm `rand` is already a workspace dep**

```bash
grep -n "^rand " src-tauri/Cargo.toml
```

If empty, also try:
```bash
grep -rn '"rand"' src-tauri/Cargo.toml
cargo tree -p uclaw 2>&1 | grep "^├── rand\|^│   ├── rand\| rand v" | head -3
```

If `rand` is not in `[dependencies]`, this plan adds it as a direct dep (Task 1 has the line). If it is, skip the Cargo.toml edit.

---

## File Structure

| Path | Action | Purpose |
|---|---|---|
| `src-tauri/src/agent/retry/mod.rs` | create | re-exports `RetryBudget`, `BudgetDecision`, error events |
| `src-tauri/src/agent/retry/backoff.rs` | create | pure `compute_delay(attempt, remaining_budget) → Duration` + jitter |
| `src-tauri/src/agent/retry/budget.rs` | create | stateful `RetryBudget` struct + `next_delay()` |
| `src-tauri/src/agent/retry/tests.rs` | create | unit tests (8 cases) |
| `src-tauri/src/agent/mod.rs` | modify | `pub mod retry;` |
| `src-tauri/src/agent/dispatcher.rs` | modify | replace `MAX_STREAM_RETRIES` + the two `tokio::time::sleep` sites with `RetryBudget::next_delay()`; emit `agent:retry` events |
| `src-tauri/src/agent/events.rs` (or inline in dispatcher) | modify | add `AgentRetryEvent` enum if uClaw has a typed events module; otherwise emit `serde_json::json!` inline |
| `src-tauri/Cargo.toml` | modify (conditional) | add `rand = "0.8"` if missing |

**Module size budget**: every new file ≤ 200 lines. `budget.rs` peaks at ~120, `backoff.rs` at ~80, `tests.rs` at ~150. `dispatcher.rs` delta ≈ +30 / -10.

---

## Task 1: Backoff Module (pure)

**Files:**
- Create: `src-tauri/src/agent/retry/backoff.rs`

A pure function. No state, no time. Takes (attempt, remaining_budget, rng) → Duration.

- [ ] **Step 1: Verify `rand` is available**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "^rand " src-tauri/Cargo.toml
```

If empty: add `rand = "0.8"` to the `[dependencies]` block of `src-tauri/Cargo.toml`. Place it alphabetically (between `r…` entries). The `Edit` tool can find a unique anchor line — `grep -B0 -A0 "^rand_chacha\|^rayon\|^regex" src-tauri/Cargo.toml` to find a near neighbor.

If `rand` is already present (e.g. as a transitive that's actually in `[dependencies]`), skip the Cargo.toml edit.

- [ ] **Step 2: Write the failing test (we'll consolidate all retry tests in `tests.rs`, but lay down two for backoff first)**

Create `src-tauri/src/agent/retry/backoff.rs` with **only** the public surface declared (so the test file can import it):

```rust
//! Pure exponential-backoff math for the agent retry loop.
//!
//! Sequence: 1s, 2s, 4s, 8s, 15s, 15s, 15s… (cap = `RETRY_MAX_DELAY_MS`).
//! Each output is multiplied by a ±20% jitter factor, then clamped to the
//! caller's `remaining_budget` so the cumulative sleep never exceeds it.

use std::time::Duration;
use rand::Rng;

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
```

- [ ] **Step 3: Wire the module into the parent**

Create `src-tauri/src/agent/retry/mod.rs`:

```rust
//! W2: agent retry-budget extension.
//!
//! See `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4.

pub mod backoff;
pub mod budget;

#[cfg(test)]
mod tests;

pub use backoff::{BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS, compute_delay};
pub use budget::{BudgetDecision, RetryBudget};
```

Edit `src-tauri/src/agent/mod.rs` to add the new module. Locate the existing `pub mod` declarations:

```bash
grep -n "^pub mod" src-tauri/src/agent/mod.rs
```

Add a line `pub mod retry;` alphabetically (likely between `prompt_*` and `tools`). Use the `Edit` tool with a unique anchor.

- [ ] **Step 4: Build to check syntax + dep resolution**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error|cannot find" | head -10
```

Expected: empty (zero errors). If the `tests` module is referenced but doesn't exist yet, the `#[cfg(test)] mod tests;` line will compile-clean because we will create it in Task 3. If a compilation error fires here about `mod tests;` not found, comment that line out for now and re-enable in Task 3.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Status MUST show ONLY (the list may vary slightly if Cargo.toml was unmodified):
```
 M src-tauri/Cargo.toml      # only if added rand
 M src-tauri/src/agent/mod.rs
?? src-tauri/src/agent/retry/
```

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/Cargo.toml src-tauri/src/agent/mod.rs src-tauri/src/agent/retry/mod.rs src-tauri/src/agent/retry/backoff.rs
cd /Users/ryanliu/Documents/uclaw && git commit -m "feat(agent): add retry backoff module (pure delay math + jitter)"
```

---

## Task 2: Budget Module (stateful)

**Files:**
- Create: `src-tauri/src/agent/retry/budget.rs`

`RetryBudget` is stateful: it tracks attempts + cumulative wait. `next_delay()` is the only public mutator. Lives for the duration of one agent-loop iteration; resets per response.

- [ ] **Step 1: Write the failing test scaffold**

For now we put **one** smoke test inline so the module compiles. The fuller test suite arrives in Task 3.

Create `src-tauri/src/agent/retry/budget.rs`:

```rust
//! Stateful retry budget for the agent loop.
//!
//! Tracks attempt count + cumulative sleep time. `next_delay()` is the only
//! mutator and returns `BudgetDecision::Sleep(d)` until the budget is gone,
//! then `BudgetDecision::Exhausted` permanently.

use std::time::Duration;
use rand::{thread_rng, RngCore};
use super::backoff::compute_delay;

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

    pub fn attempts(&self) -> u32 { self.attempts }
    pub fn max_attempts(&self) -> u32 { self.max_attempts }
    pub fn elapsed_wait(&self) -> Duration { self.elapsed_wait }
    pub fn max_total_wait(&self) -> Duration { self.max_total_wait }

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
```

- [ ] **Step 2: Build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: empty.

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
?? src-tauri/src/agent/retry/budget.rs
```

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/agent/retry/budget.rs
cd /Users/ryanliu/Documents/uclaw && git commit -m "feat(agent): add RetryBudget struct — 25-attempt / 5-min cumulative window"
```

---

## Task 3: Tests for backoff + budget

**Files:**
- Create: `src-tauri/src/agent/retry/tests.rs`

8 test cases that pin the contract.

- [ ] **Step 1: Write the tests**

Create `src-tauri/src/agent/retry/tests.rs`:

```rust
use super::backoff::{compute_delay, BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS};
use super::budget::{BudgetDecision, MAX_AUTO_RETRIES, MAX_AUTO_RETRY_WAIT_MS, RetryBudget};
use rand::rngs::mock::StepRng;
use std::time::Duration;

// A deterministic RNG that always returns the SAME value, used to make
// jitter testable. `gen_range(-0.2..=0.2)` on StepRng(0, 0) returns -0.2.
// On StepRng(u64::MAX, 0) it returns +0.2. We don't need that level of
// precision — we just need the boundaries.
fn rng_zero_jitter() -> StepRng { StepRng::new(u64::MAX / 2, 0) }
fn rng_min_jitter() -> StepRng { StepRng::new(0, 0) }
fn rng_max_jitter() -> StepRng { StepRng::new(u64::MAX, 0) }

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
    assert!((d1.as_millis() as i64 - 1_000).abs() < 20, "attempt 1: {:?}", d1);
    assert!((d2.as_millis() as i64 - 2_000).abs() < 20, "attempt 2: {:?}", d2);
    assert!((d3.as_millis() as i64 - 4_000).abs() < 20, "attempt 3: {:?}", d3);
    assert!((d4.as_millis() as i64 - 8_000).abs() < 20, "attempt 4: {:?}", d4);
    assert!((d5.as_millis() as i64 - 15_000).abs() < 20, "attempt 5: {:?}", d5);
    assert!((d6.as_millis() as i64 - 15_000).abs() < 20, "attempt 6 (capped): {:?}", d6);
}

#[test]
fn compute_delay_min_jitter_floors_at_minus_20_percent() {
    let d = compute_delay(5, Duration::from_secs(3600), &mut rng_min_jitter());
    // 15s * 0.8 = 12s
    assert!(d.as_millis() >= 11_900 && d.as_millis() <= 12_100, "got {:?}", d);
}

#[test]
fn compute_delay_max_jitter_ceils_at_plus_20_percent() {
    let d = compute_delay(5, Duration::from_secs(3600), &mut rng_max_jitter());
    // 15s * 1.2 = 18s — but clamped to remaining budget (3600s here, so unaffected)
    assert!(d.as_millis() >= 17_900 && d.as_millis() <= 18_100, "got {:?}", d);
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
    assert!(matches!(b.next_delay_with(&mut rng), BudgetDecision::Sleep(_)));
    assert!(matches!(b.next_delay_with(&mut rng), BudgetDecision::Sleep(_)));
    assert!(matches!(b.next_delay_with(&mut rng), BudgetDecision::Sleep(_)));
    assert_eq!(b.next_delay_with(&mut rng), BudgetDecision::Exhausted);
    assert_eq!(b.attempts(), 3);
}

#[test]
fn budget_returns_exhausted_when_time_runs_out() {
    // 2000ms budget. With zero jitter, attempt 1 = 1000ms, attempt 2 = 1000ms (clamped). Then exhausted.
    let mut b = RetryBudget::with_limits(99, 2_000);
    let mut rng = rng_zero_jitter();
    let d1 = b.next_delay_with(&mut rng);
    let d2 = b.next_delay_with(&mut rng);
    let d3 = b.next_delay_with(&mut rng);
    assert!(matches!(d1, BudgetDecision::Sleep(_)));
    assert!(matches!(d2, BudgetDecision::Sleep(_)));
    assert_eq!(d3, BudgetDecision::Exhausted, "third call should be exhausted, got {:?}", d3);
    assert!(b.elapsed_wait() <= Duration::from_millis(2_000));
}

#[test]
fn budget_default_for_agent_loop_uses_proma_constants() {
    let b = RetryBudget::for_agent_loop();
    assert_eq!(b.max_attempts(), MAX_AUTO_RETRIES);
    assert_eq!(b.max_attempts(), 25);
    assert_eq!(b.max_total_wait(), Duration::from_millis(MAX_AUTO_RETRY_WAIT_MS));
    assert_eq!(b.max_total_wait(), Duration::from_secs(300));
    assert_eq!(b.elapsed_wait(), Duration::ZERO);
    assert_eq!(b.attempts(), 0);
}

// Suppress unused-import warnings from constants we re-export but don't directly assert on
#[allow(dead_code)]
const _SANITY: (u64, u64, f64) = (BASE_DELAY_MS, RETRY_MAX_DELAY_MS, JITTER_RATIO);
```

- [ ] **Step 2: Run the tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib agent::retry 2>&1 | tail -15
```

Expected: 8 tests pass.

If `rand::rngs::mock` is not available, replace the StepRng with a simple custom `RngCore` that returns a fixed `u64` (the `rand` `mock` module was promoted to `rand::rngs::mock` only after `0.8.0`; in older versions it lives in `rand_core`). Verify the path via `cargo doc --open -p rand` or just import-test with:

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo check --lib 2>&1 | grep -A2 "mock"
```

If the import fails, use this minimal fallback at the top of `tests.rs`:

```rust
struct FixedRng(u64);
impl rand::RngCore for FixedRng {
    fn next_u32(&mut self) -> u32 { self.0 as u32 }
    fn next_u64(&mut self) -> u64 { self.0 }
    fn fill_bytes(&mut self, dst: &mut [u8]) { rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, dst) }
    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), rand::Error> { Ok(rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, dst)) }
}
fn rng_zero_jitter() -> FixedRng { FixedRng(u64::MAX / 2) }
fn rng_min_jitter() -> FixedRng { FixedRng(0) }
fn rng_max_jitter() -> FixedRng { FixedRng(u64::MAX) }
```

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
?? src-tauri/src/agent/retry/tests.rs
```

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/agent/retry/tests.rs
cd /Users/ryanliu/Documents/uclaw && git commit -m "test(agent): retry-budget unit tests — backoff sequence + jitter + exhaustion paths"
```

---

## Task 4: IPC event type for retries

**Files:**
- Modify: `src-tauri/src/agent/retry/mod.rs` (add the event payload type)

The dispatcher will `emit("agent:retry", &payload)`. We define the payload here so the type is reusable and self-documenting.

- [ ] **Step 1: Add the event type to `retry/mod.rs`**

Replace the existing `retry/mod.rs` with this expanded version (keeps everything you wrote in Task 1 + adds the event payload + Serialize):

```rust
//! W2: agent retry-budget extension.
//!
//! See `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §4.

pub mod backoff;
pub mod budget;

#[cfg(test)]
mod tests;

pub use backoff::{BASE_DELAY_MS, JITTER_RATIO, RETRY_MAX_DELAY_MS, compute_delay};
pub use budget::{BudgetDecision, RetryBudget};

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
```

- [ ] **Step 2: Build**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

Expected: empty. (`serde` and `serde::Serialize` are already in uClaw's tree — they're used elsewhere.)

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/agent/retry/mod.rs
cd /Users/ryanliu/Documents/uclaw && git commit -m "feat(agent): add AgentRetryEvent payload type for agent:retry IPC channel"
```

---

## Task 5: Wire RetryBudget into dispatcher

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs`

Two retry sites both consult the shared budget. The existing 2-retry / 500ms-base loop gets replaced.

- [ ] **Step 1: Read the current retry sites**

```bash
cd /Users/ryanliu/Documents/uclaw && grep -n "MAX_STREAM_RETRIES\|stream_retries\|'stream_attempt" src-tauri/src/agent/dispatcher.rs
```

Note the line numbers of:
- The const definition (currently `MAX_STREAM_RETRIES: u32 = 2` near line 18)
- The `stream_retries` local variable initialization inside the function
- Both `tokio::time::sleep(...)` call sites
- The `'stream_attempt: loop` label

- [ ] **Step 2: Remove the const, add the use**

In `src-tauri/src/agent/dispatcher.rs`:

Find the line:
```rust
const MAX_STREAM_RETRIES: u32 = 2;
```

Replace with:
```rust
use crate::agent::retry::{AgentRetryEvent, BudgetDecision, RetryBudget};
```

(Note: the comment block above the const is preserved — only remove the `const ...;` line.)

Then in the imports block at the top of the file (near `use crate::agent::types::*;`), ensure these are not duplicated.

- [ ] **Step 3: Initialize the budget at the start of `respond_with_stream` (or whichever function houses the `'stream_attempt: loop`)**

Locate the function that contains `'stream_attempt: loop` (likely a method on `ChatDelegate`). Just BEFORE the `let mut stream_retries = 0u32;` line (or the local that tracks retries), add:

```rust
            let mut retry_budget = RetryBudget::for_agent_loop();
```

If `stream_retries` is the only retry-related local, REMOVE that line — `retry_budget.attempts()` replaces it. If `stream_retries` is consulted in event emission or logging, change those reads to `retry_budget.attempts()`.

- [ ] **Step 4: Rewrite the mid-stream retry block (around line 615)**

Find the existing pattern:
```rust
                                match kind {
                                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork
                                        if stream_retries < MAX_STREAM_RETRIES =>
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            kind = ?kind,
                                            attempt = stream_retries + 1,
                                            max = MAX_STREAM_RETRIES,
                                            "Stream interrupted, retrying with a fresh stream"
                                        );
                                        self.emit_stream_reset();
                                        stream_retries += 1;
                                        // Brief backoff before retry
                                        tokio::time::sleep(std::time::Duration::from_millis(
                                            500 * 2u64.pow(stream_retries - 1),
                                        )).await;
                                        continue 'stream_attempt;
                                    }
                                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                        tracing::error!(
                                            error = %e,
                                            retries = stream_retries,
                                            "Stream failed after exhausting retries"
                                        );
                                        self.emit_stream_reset();
                                        return Err(e);
                                    }
                                    StreamErrorKind::Fatal => {
                                        tracing::error!(error = %e, "Stream failed with fatal error");
                                        self.emit_stream_reset();
                                        return Err(e);
                                    }
                                }
```

Replace with:

```rust
                                match kind {
                                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                        let decision = retry_budget.next_delay();
                                        match decision {
                                            BudgetDecision::Sleep(delay) => {
                                                let reason = format!("{:?}: {}", kind, e);
                                                tracing::warn!(
                                                    error = %e,
                                                    kind = ?kind,
                                                    attempt = retry_budget.attempts(),
                                                    max = retry_budget.max_attempts(),
                                                    delay_ms = delay.as_millis() as u64,
                                                    "Stream interrupted, retrying with a fresh stream"
                                                );
                                                self.emit_stream_reset();
                                                self.emit_retry_event(AgentRetryEvent::Starting {
                                                    attempt: retry_budget.attempts(),
                                                    max_attempts: retry_budget.max_attempts(),
                                                    delay_seconds: delay.as_secs_f64(),
                                                    reason: reason.clone(),
                                                });

                                                // Sleep, but abort early if the session is stopped.
                                                if self.sleep_or_abort(delay).await {
                                                    return Err(e);
                                                }

                                                self.emit_retry_event(AgentRetryEvent::Attempt {
                                                    attempt: retry_budget.attempts(),
                                                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                                    reason,
                                                });
                                                continue 'stream_attempt;
                                            }
                                            BudgetDecision::Exhausted => {
                                                tracing::error!(
                                                    error = %e,
                                                    attempts = retry_budget.attempts(),
                                                    elapsed_wait_ms = retry_budget.elapsed_wait().as_millis() as u64,
                                                    "Stream failed after exhausting retry budget"
                                                );
                                                self.emit_stream_reset();
                                                self.emit_retry_event(AgentRetryEvent::Exhausted {
                                                    total_attempts: retry_budget.attempts(),
                                                    total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                                });
                                                return Err(e);
                                            }
                                        }
                                    }
                                    StreamErrorKind::Fatal => {
                                        tracing::error!(error = %e, "Stream failed with fatal error");
                                        self.emit_stream_reset();
                                        return Err(e);
                                    }
                                }
```

- [ ] **Step 5: Rewrite the stream-setup retry block (around line 681)**

Find the existing pattern (after the second `Err(e)` in `match stream_result`):
```rust
                    let kind = classify_stream_error(&e);
                    match kind {
                        StreamErrorKind::TransientNetwork
                            if stream_retries < MAX_STREAM_RETRIES =>
                        {
                            tracing::warn!(
                                error = %e,
                                attempt = stream_retries + 1,
                                "Stream setup failed transiently, retrying"
                            );
                            stream_retries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * 2u64.pow(stream_retries - 1),
                            )).await;
                            continue 'stream_attempt;
                        }
                        _ => {
                            tracing::error!(error = %e, "Stream setup failed, surfacing error");
                            return Err(e);
                        }
                    }
```

Replace with:

```rust
                    let kind = classify_stream_error(&e);
                    match kind {
                        StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                            let decision = retry_budget.next_delay();
                            match decision {
                                BudgetDecision::Sleep(delay) => {
                                    let reason = format!("setup {:?}: {}", kind, e);
                                    tracing::warn!(
                                        error = %e,
                                        kind = ?kind,
                                        attempt = retry_budget.attempts(),
                                        max = retry_budget.max_attempts(),
                                        delay_ms = delay.as_millis() as u64,
                                        "Stream setup failed transiently, retrying"
                                    );
                                    self.emit_retry_event(AgentRetryEvent::Starting {
                                        attempt: retry_budget.attempts(),
                                        max_attempts: retry_budget.max_attempts(),
                                        delay_seconds: delay.as_secs_f64(),
                                        reason: reason.clone(),
                                    });
                                    if self.sleep_or_abort(delay).await {
                                        return Err(e);
                                    }
                                    self.emit_retry_event(AgentRetryEvent::Attempt {
                                        attempt: retry_budget.attempts(),
                                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                        reason,
                                    });
                                    continue 'stream_attempt;
                                }
                                BudgetDecision::Exhausted => {
                                    tracing::error!(
                                        error = %e,
                                        attempts = retry_budget.attempts(),
                                        "Stream setup failed after exhausting retry budget"
                                    );
                                    self.emit_retry_event(AgentRetryEvent::Exhausted {
                                        total_attempts: retry_budget.attempts(),
                                        total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                    });
                                    return Err(e);
                                }
                            }
                        }
                        StreamErrorKind::Fatal => {
                            tracing::error!(error = %e, "Stream setup failed, surfacing error");
                            return Err(e);
                        }
                    }
```

(Note: this expands the setup retry to also handle `Stalled` for consistency with the mid-stream site, even though setup is unlikely to stall — harmless symmetry.)

- [ ] **Step 6: Add the `emit_retry_event` + `sleep_or_abort` helper methods on `ChatDelegate`**

Locate the impl block for `ChatDelegate` (likely `impl ChatDelegate {`). Find the existing `emit_stream_reset` method as a placement anchor:

```bash
grep -n "fn emit_stream_reset\|impl ChatDelegate" src-tauri/src/agent/dispatcher.rs | head -5
```

Add these two methods next to `emit_stream_reset`:

```rust
    /// Emit the `agent:retry` IPC event. Failures are non-fatal — we only
    /// log, so the retry loop is never blocked by a Tauri emit error.
    fn emit_retry_event(&self, event: AgentRetryEvent) {
        if let Err(e) = self.app_handle.emit(AgentRetryEvent::CHANNEL, &event) {
            tracing::debug!(error = %e, "Failed to emit agent:retry event");
        }
    }

    /// Sleep for `duration`, but wake up early if the session's stop flag
    /// flips. Returns `true` if the wake was triggered by the stop flag
    /// (caller should bail), `false` if the full duration elapsed.
    async fn sleep_or_abort(&self, duration: std::time::Duration) -> bool {
        let stop = self.stop_flag.clone();
        tokio::select! {
            _ = tokio::time::sleep(duration) => false,
            _ = async {
                // Poll the stop flag every 100ms. Cheap; bounded by the retry delay.
                loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            } => true,
        }
    }
```

If `tauri::Emitter` is not yet imported in this file, add `use tauri::Emitter;` at the top (check first; you saw it earlier near `use tauri::Emitter;`).

- [ ] **Step 7: Build + unit tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -10
```

Expected: build clean, all tests pass (the agent::retry tests from Task 3 still pass; the existing test suite still passes).

- [ ] **Step 8: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
```

Expected:
```
 M src-tauri/src/agent/dispatcher.rs
```

```bash
cd /Users/ryanliu/Documents/uclaw && git add src-tauri/src/agent/dispatcher.rs
cd /Users/ryanliu/Documents/uclaw && git commit -m "feat(agent): wire RetryBudget + sleep_or_abort into stream retry sites

Replaces the hard-coded MAX_STREAM_RETRIES=2 / 500ms-base loop with the
shared 25-attempt / 5-min / 15s-cap / ±20%-jitter budget. Both the
mid-stream retry (line ~615) and the stream-setup retry (line ~681) now
consult the same RetryBudget instance per request, so cumulative sleep
respects the 5-minute cap.

Adds emit_retry_event helper publishing agent:retry IPC events
(Starting/Attempt/Exhausted) and sleep_or_abort helper that wakes early
if the session's stop_flag flips during a long backoff."
```

---

## Task 6: Frontend listener (smoke)

**Files:**
- (Optional) Read `ui/src/hooks/useGlobalAgentListeners.ts` to identify where the `agent:retry` listener should land in a future UI task.

W2 does NOT add UI for the retry banner — that's W4's job (the Preview Engine spec already plans to consume `agent:retry`). For W2 we only confirm the event reaches a renderable surface.

- [ ] **Step 1: Manual smoke after the PR is open**

Add to the PR description's manual-test section:
- With network disconnected, send a message → confirm DevTools console shows `agent:retry` events with `status: "starting"`, then `status: "attempt"`, repeating for ~5 min before final fail
- Stop the session mid-retry → confirm next event is `status: "exhausted"` (or the loop exits without further sleep)

No code edits in this task; it's a checklist item.

- [ ] **Step 2: No commit needed**

---

## Task 7: Final verification + push + PR

- [ ] **Step 1: Full Rust suite**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -10
```

Expected: all green. Test count up by 8 (the new retry-module tests).

- [ ] **Step 2: TS + frontend tests** (sanity — we don't touch ui/, but confirm)

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: no regressions.

- [ ] **Step 3: Git log review**

```bash
cd /Users/ryanliu/Documents/uclaw && git log --oneline main..HEAD
```

Expected commits in order:

1. `feat(agent): add retry backoff module (pure delay math + jitter)`
2. `feat(agent): add RetryBudget struct — 25-attempt / 5-min cumulative window`
3. `test(agent): retry-budget unit tests — backoff sequence + jitter + exhaustion paths`
4. `feat(agent): add AgentRetryEvent payload type for agent:retry IPC channel`
5. `feat(agent): wire RetryBudget + sleep_or_abort into stream retry sites`

(5 bisectable commits.)

- [ ] **Step 4: Push and open PR**

```bash
cd /Users/ryanliu/Documents/uclaw && git push -u origin claude/w2-agent-retry-budget
cd /Users/ryanliu/Documents/uclaw && gh pr create --title "W2: agent retry budget (25 attempts / 5 min / jitter)" --body "$(cat <<'EOF'
## Summary

Wave 2 of the [Proma v0.9.27 preview port](docs/superpowers/specs/2026-05-12-proma-preview-port-design.md) (see §4). Replaces uClaw's hard-coded `MAX_STREAM_RETRIES = 2` / 500ms-base / no-jitter / no-cumulative-cap retry policy with a shared `RetryBudget` modeled on Proma PR #419:

| Constant | Old | New |
|---|---|---|
| max attempts | 2 | **25** |
| backoff cap | n/a (no cap, raw doubling) | **15s** |
| cumulative cap | none | **5 min** |
| jitter | none | **±20%** |
| sleep abortable on session-stop | no | **yes** (`sleep_or_abort`) |

Both `dispatcher.rs` retry sites (mid-stream + stream-setup) now consult the same `RetryBudget` instance per request. Backup-tolerance (§4.2 of the spec) is **dropped**: uClaw has no comparable backup/export feature, so YAGNI.

Adds a new `agent:retry` IPC event channel that W4's Preview Engine will surface in the UI. W2 only emits — UI consumption is W4.

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1 | `feat(agent): add retry backoff module (pure delay math + jitter)` | `agent/retry/backoff.rs` |
| 2 | `feat(agent): add RetryBudget struct — 25-attempt / 5-min cumulative window` | `agent/retry/budget.rs` |
| 3 | `test(agent): retry-budget unit tests` | 8 cases pinning sequence + boundaries |
| 4 | `feat(agent): add AgentRetryEvent payload type for agent:retry IPC channel` | typed event |
| 5 | `feat(agent): wire RetryBudget + sleep_or_abort into stream retry sites` | wiring + helpers |

## Test plan

- [x] `cd src-tauri && cargo build` — clean
- [x] `cd src-tauri && cargo test --lib agent::retry` — 8/8 pass
- [x] `cd src-tauri && cargo test --lib` — full suite clean
- [x] `cd ui && npx tsc --noEmit && npm test -- --run` — no regressions (no UI changes)
- [ ] Manual: kill network mid-stream → DevTools console emits `agent:retry` `status: starting` / `attempt` cycles for ~5 min then a single `status: exhausted` before the surfaced error toast
- [ ] Manual: stop session mid-retry → loop exits at next `sleep_or_abort` checkpoint
- [ ] Manual: short 429 burst → retries succeed, banner clears, response delivered

## Out of scope (future)

- UI banner / retry counter — W4 will consume `agent:retry` (see preview-port spec §4.3)
- Backup / export tolerance — uClaw has no such feature today; not in scope
EOF
)"
```

Expected: PR URL printed.

---

## Self-Review (run mentally before handoff)

**Spec coverage** — each spec §4.x maps to a task:

| Spec | Task |
|---|---|
| §4.1 constants (`MAX_AUTO_RETRIES=25`, `MAX_AUTO_RETRY_WAIT_MS=300_000`, `RETRY_MAX_DELAY_MS=15_000`, jitter ±20%) | Tasks 1, 2 (constants live in those files) |
| §4.1 modules (`mod.rs`, `budget.rs`, `backoff.rs`, `tests.rs`) | Tasks 1, 2, 3, 4 |
| §4.1 wiring (`agent/agentic_loop.rs`) | Task 5 — note: in uClaw the relevant file is `agent/dispatcher.rs`, not `agentic_loop.rs`. Both files exist; the retry sites live in dispatcher. The spec is being updated to reflect this. |
| §4.1 IPC events | Tasks 4, 5 |
| §4.1 abort-during-sleep | Task 5 (`sleep_or_abort`) |
| §4.2 backup tolerance | YAGNI'd (confirmed via grep) — spec §4.2 will be updated to record path-B |
| §4.3 event payload contract | Task 4 |

**Type consistency:**
- `RetryBudget::next_delay() → BudgetDecision` — used identically in Task 5's two call sites.
- `AgentRetryEvent::CHANNEL = "agent:retry"` — single source for the event name.
- `BASE_DELAY_MS = 1000`, `RETRY_MAX_DELAY_MS = 15_000`, `JITTER_RATIO = 0.2` — constants exported from `backoff.rs`, consumed by `budget.rs` via `compute_delay`.
- `MAX_AUTO_RETRIES = 25`, `MAX_AUTO_RETRY_WAIT_MS = 300_000` — constants in `budget.rs`, consumed in `RetryBudget::for_agent_loop`.

**Placeholder scan:** none — every code step contains complete code.

**Module size:**
- `backoff.rs`: ~50 lines ✓
- `budget.rs`: ~80 lines ✓
- `mod.rs`: ~45 lines ✓ (after Task 4 expansion)
- `tests.rs`: ~120 lines ✓
- `dispatcher.rs` delta: +~60 / -~25 ✓

All under 300-line target.

**Risk surfaces flagged for review:**

1. **Stop-flag granularity in `sleep_or_abort`**: polls every 100ms. A long-running retry sleeps for up to 15s — worst case the user clicks "stop" and waits 100ms before the loop notices. Acceptable.
2. **`rand` dep**: if not in the workspace already, Task 1 adds it. Verify Cargo.lock churn is minimal in code review.
3. **Both retry sites now share a single `retry_budget`**: a request that fails setup AND mid-stream will consume from the same 5-min pool. This matches Proma's behavior and is the desired semantics (one budget per request).
4. **The setup retry was previously gated on `TransientNetwork` only**; Task 5 expands it to also handle `Stalled` for symmetry. If a setup-time stall is impossible (no stream yet → nothing to stall), this is harmless. If it IS possible, this is a small behavior improvement, not a regression.

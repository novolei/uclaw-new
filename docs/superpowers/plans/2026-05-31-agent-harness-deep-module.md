# AgentHarness Deep Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `AgentHarness` the Deep Module for the first run lifecycle slice: context preparation, cancellation-token installation, timeout/cancellation selection, and task hook dispatch.

**Architecture:** Keep the existing `run_agent_harness` Interface for this slice and deepen the Implementation behind it. Production callers should cross the harness seam instead of calling `run_agentic_loop` directly. `agentic_loop` remains the low-level Implementation and its tests may keep direct calls.

**Tech Stack:** Rust, Tokio, existing `LoopDelegate`, `ReasoningContext`, `HookBus`, `CancellationToken`, Superpowers TDD.

---

## Source-of-Truth References

- Spec: `docs/superpowers/specs/2026-05-31-agent-harness-deep-module-design.md`
- Parent spec: `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
- Current shallow harness: `src-tauri/src/agent/harness.rs`
- Current timeout/hook helper: `src-tauri/src/agent/run_assembly.rs`
- Current direct production callsites:

```bash
rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime
```

## Required GitNexus Checks

Before editing production code, run impact analysis on:

- `run_agent_harness`
- `run_agent`
- `RegularTask::run`
- `run_with_rollout`
- `run_worker`

Record risk level, direct callers, and affected processes in the execution
notes. If any result is HIGH or CRITICAL, report before editing that symbol.

## Task 1: Add failing harness tests

**Files:**
- Modify: `src-tauri/src/agent/harness.rs`

- [ ] **Step 1: Add test scaffolding to `harness.rs`**

Append this `#[cfg(test)]` module to `src-tauri/src/agent/harness.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::hook_bus::{HookEvent, HookEventKind, HookSubscriber, SubscriberId};
    use crate::agent::types::{
        AgenticLoopConfig, LoopDelegate, LoopOutcome, LoopSignal, ReasoningContext,
        RespondOutput, ResponseMetadata, TextAction, TokenUsage,
    };
    use crate::error::Error;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingDelegate {
        observed_force_text: Mutex<Vec<bool>>,
        observed_token_present: Mutex<Vec<bool>>,
    }

    #[async_trait]
    impl LoopDelegate for RecordingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }

        async fn before_llm_call(
            &self,
            reason_ctx: &mut ReasoningContext,
            _iteration: usize,
        ) -> Option<LoopOutcome> {
            self.observed_force_text.lock().unwrap().push(reason_ctx.force_text);
            self.observed_token_present
                .lock()
                .unwrap()
                .push(reason_ctx.cancellation_token.is_some());
            Some(LoopOutcome::Response {
                text: "done".to_string(),
                usage: None,
                model: None,
            })
        }

        async fn call_llm(
            &self,
            _reason_ctx: &mut ReasoningContext,
            _snapshot: &crate::agent::turn::TurnSnapshot,
            _iteration: usize,
        ) -> Result<RespondOutput, Error> {
            unreachable!("before_llm_call returns a terminal outcome")
        }

        async fn handle_text_response(
            &self,
            _text: &str,
            _metadata: ResponseMetadata,
            _reason_ctx: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }

        async fn execute_tool_calls(
            &self,
            _tool_calls: Vec<crate::agent::types::ToolCall>,
            _reason_ctx: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, Error> {
            Ok(None)
        }
    }

    struct RecordingHookSubscriber {
        events: Arc<Mutex<Vec<HookEvent>>>,
    }

    #[async_trait]
    impl HookSubscriber for RecordingHookSubscriber {
        fn id(&self) -> SubscriberId {
            SubscriberId::new("harness-test")
        }

        fn interest_in(&self) -> &'static [HookEventKind] {
            &[HookEventKind::TaskStart, HookEventKind::TaskEnd]
        }

        async fn on_event(
            &self,
            event: &HookEvent,
        ) -> Option<crate::runtime::contracts::HookDecision> {
            self.events.lock().unwrap().push(event.clone());
            None
        }
    }

    fn config() -> AgenticLoopConfig {
        AgenticLoopConfig { max_iterations: 1 }
    }

    fn run_config(task_id: &str) -> AgentHarnessRunConfig {
        AgentHarnessRunConfig {
            task_id: task_id.to_string(),
            timeout_secs: 5,
        }
    }

    fn hook_bus_with_recorder() -> (Arc<HookBus>, Arc<Mutex<Vec<HookEvent>>>) {
        let mut bus = HookBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        bus.register(Arc::new(RecordingHookSubscriber { events: events.clone() }))
            .unwrap();
        (Arc::new(bus), events)
    }
}
```

- [ ] **Step 2: Add failing test for context preparation**

Inside the same module, add:

```rust
#[tokio::test]
async fn harness_resets_force_text_and_installs_cancellation_token() {
    let delegate = RecordingDelegate::default();
    let mut ctx = ReasoningContext::new("system".to_string());
    ctx.force_text = true;
    let (hook_bus, _events) = hook_bus_with_recorder();
    let token = CancellationToken::new();

    let outcome = run_agent_harness(
        &delegate,
        &mut ctx,
        &config(),
        token,
        hook_bus,
        run_config("task-1"),
    )
    .await;

    assert!(matches!(outcome, AgentHarnessRunOutcome::Completed(_)));
    assert_eq!(*delegate.observed_force_text.lock().unwrap(), vec![false]);
    assert_eq!(*delegate.observed_token_present.lock().unwrap(), vec![true]);
}
```

- [ ] **Step 3: Add failing test for hook events**

Inside the same module, add:

```rust
#[tokio::test]
async fn harness_dispatches_task_start_and_end_once() {
    let delegate = RecordingDelegate::default();
    let mut ctx = ReasoningContext::new("system".to_string());
    let (hook_bus, events) = hook_bus_with_recorder();

    let outcome = run_agent_harness(
        &delegate,
        &mut ctx,
        &config(),
        CancellationToken::new(),
        hook_bus,
        run_config("task-2"),
    )
    .await;

    assert!(matches!(outcome, AgentHarnessRunOutcome::Completed(_)));
    let events = events.lock().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0], HookEvent::TaskStart { task_id, .. } if task_id == "task-2"));
    assert!(matches!(&events[1], HookEvent::TaskEnd { task_id, outcome } if task_id == "task-2" && outcome == "completed"));
}
```

- [ ] **Step 4: Run tests to verify RED**

Run:

```bash
cd src-tauri && cargo test --lib agent::harness -- --nocapture
```

Expected: failure showing `observed_force_text` saw `true` or
`observed_token_present` saw `false`, because the current harness delegates to
`run_assembly` without preparing the context.

## Task 2: Deepen `run_agent_harness`

**Files:**
- Modify: `src-tauri/src/agent/harness.rs`
- Possible modify: `src-tauri/src/agent/run_assembly.rs`

- [ ] **Step 1: Implement minimal context preparation in `run_agent_harness`**

Before calling `run_assembly::run_agent`, add:

```rust
    ctx.force_text = false;
    ctx.cancellation_token = Some(token.clone());
```

Keep the existing call to `run_assembly::run_agent` unchanged.

- [ ] **Step 2: Run harness tests to verify GREEN**

Run:

```bash
cd src-tauri && cargo test --lib agent::harness -- --nocapture
```

Expected: the two new harness tests pass.

- [ ] **Step 3: Run run_assembly tests**

Run:

```bash
cd src-tauri && cargo test --lib agent::run_assembly -- --nocapture
```

Expected: existing run assembly tests pass.

## Task 3: Migrate `RegularTask::run` to the harness seam

**Files:**
- Modify: `src-tauri/src/agent/regular_task.rs`

- [ ] **Step 1: Run GitNexus impact for `RegularTask::run`**

Run the GitNexus impact tool for `RegularTask::run` with upstream direction.
Record risk and direct callers in execution notes.

- [ ] **Step 2: Write failing test or adapt existing regular task test**

Find the existing `RegularTask` tests:

```bash
rg -n "RegularTask::new|RegularTask|force_text|cancellation_token" src-tauri/src/agent/regular_task.rs src-tauri/src/agent/regular_task_pr4_tests.rs
```

Add or adapt a test proving `RegularTask::run` receives harness preparation by
using a delegate that records `force_text` and token presence at loop entry.

- [ ] **Step 3: Verify RED**

Run:

```bash
cd src-tauri && cargo test --lib agent::regular_task -- --nocapture
```

Expected: new/changed test fails before migration because the assertion should
be written against the harness path, not the duplicated direct path.

- [ ] **Step 4: Add HookBus to `RegularTaskInputs`**

Add the harness dependency to `RegularTaskInputs`:

```rust
pub struct RegularTaskInputs {
    pub delegate: Arc<dyn LoopDelegate>,
    pub reason_ctx: Arc<Mutex<ReasoningContext>>,
    pub config: AgenticLoopConfig,
    pub hook_bus: Arc<crate::agent::hook_bus::HookBus>,
}
```

Update every `RegularTaskInputs { ... }` fixture in `regular_task.rs` and
`regular_task_pr4_tests.rs` with:

```rust
hook_bus: Arc::new(crate::agent::hook_bus::HookBus::new()),
```

- [ ] **Step 5: Replace local setup and direct call**

In `RegularTask::run`, replace:

```rust
ctx.force_text = false;
ctx.cancellation_token = Some(token.clone());

let outcome = run_agentic_loop(
    self.inputs.delegate.as_ref(),
    &mut ctx,
    &self.inputs.config,
)
.await;
```

with:

```rust
let outcome = crate::agent::harness::run_agent_harness(
    self.inputs.delegate.as_ref(),
    &mut ctx,
    &self.inputs.config,
    token.clone(),
    self.inputs.hook_bus.clone(),
    crate::agent::harness::AgentHarnessRunConfig {
        task_id: self.spec.id.clone(),
        timeout_secs: self.spec.budget.max_wallclock_seconds.unwrap_or(300),
    },
)
.await;

let outcome = match outcome {
    crate::agent::harness::AgentHarnessRunOutcome::Completed(outcome) => outcome,
    crate::agent::harness::AgentHarnessRunOutcome::TimedOut => LoopOutcome::Failure {
        error: "task timed out".to_string(),
    },
    crate::agent::harness::AgentHarnessRunOutcome::Cancelled => LoopOutcome::Cancelled {
        partial_code: None,
    },
};
```

If the 300-second fallback appears in more than one place, add a helper constant
near `RegularTaskInputs`:

```rust
const DEFAULT_HARNESS_TIMEOUT_SECS: u64 = 300;
```

and use it in the `AgentHarnessRunConfig`.

- [ ] **Step 6: Verify GREEN**

Run:

```bash
cd src-tauri && cargo test --lib agent::regular_task -- --nocapture
```

Expected: regular task tests pass.

## Task 4: Classify remaining direct low-level callsites

**Files:**
- Modify: `src-tauri/src/agent/rollout_integration.rs`
- Modify: `src-tauri/src/agent/teams/worker.rs`
- Possible modify: context structs that need `HookBus`

- [ ] **Step 1: Re-run callsite search**

Run:

```bash
rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime
```

Expected: direct production callsites are `rollout_integration.rs` and
`teams/worker.rs`, plus low-level tests in `agentic_loop.rs`.

- [ ] **Step 2: Migrate callsites that can access `HookBus`**

For each production callsite with a `HookBus`, replace direct
`run_agentic_loop` with `run_agent_harness`. Use the same outcome mapping as
Task 3.

- [ ] **Step 3: Document deferred low-level adapters**

If a callsite cannot access `HookBus` without broad constructor churn, add a
short comment immediately above the direct call:

```rust
// AgentHarness migration note: this adapter remains low-level until the
// rollout/team harness adapter threads HookBus through the owning context.
// It must not grow new lifecycle behaviour beyond rollout/team event bridging.
```

This comment is acceptable only for `rollout_integration.rs` or
`teams/worker.rs`, and only in this first slice.

- [ ] **Step 4: Verify search result**

Run:

```bash
rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime
```

Expected: remaining direct calls are `agentic_loop.rs` tests/internal low-level
Implementation, `run_assembly.rs` internal harness helper, and at most
documented rollout/team adapters.

## Task 5: Focused verification and commit

**Files:**
- All touched files from Tasks 1-4

- [ ] **Step 1: Run focused tests**

Run:

```bash
cd src-tauri && cargo test --lib agent::harness -- --nocapture
cd src-tauri && cargo test --lib agent::run_assembly -- --nocapture
cd src-tauri && cargo test --lib agent::regular_task -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run search verification**

Run:

```bash
rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime
rg -n "ctx\\.force_text = false|cancellation_token = Some\\(" src-tauri/src/agent/regular_task.rs src-tauri/src/agent/harness.rs
```

Expected: regular task no longer owns the context-preparation lines; harness
does.

- [ ] **Step 3: Run GitNexus detect-changes**

Run GitNexus detect-changes.

Expected: changed symbols match the harness/regular-task scope.

- [ ] **Step 4: Commit**

Run:

```bash
git add src-tauri/src/agent/harness.rs src-tauri/src/agent/run_assembly.rs src-tauri/src/agent/regular_task.rs src-tauri/src/agent/rollout_integration.rs src-tauri/src/agent/teams/worker.rs docs/superpowers/specs/2026-05-31-agent-harness-deep-module-design.md docs/superpowers/plans/2026-05-31-agent-harness-deep-module.md
git commit -m "refactor(agent): deepen AgentHarness run lifecycle seam" -m "Verification: cd src-tauri && cargo test --lib agent::harness -- --nocapture; cd src-tauri && cargo test --lib agent::run_assembly -- --nocapture; cd src-tauri && cargo test --lib agent::regular_task -- --nocapture"
```

Expected: commit succeeds without bypassing hooks.

## Task 6: Code review gate

- [ ] **Step 1: Request code review**

Use the requesting-code-review skill with:

- Description: Deepened AgentHarness first run lifecycle slice.
- Requirements: This plan and `2026-05-31-agent-harness-deep-module-design.md`.
- Base SHA: commit before Task 1.
- Head SHA: current HEAD.

- [ ] **Step 2: Fix Critical and Important findings**

Run the same focused tests after any fix.

- [ ] **Step 3: Mark child project complete**

Update the umbrella plan checkboxes for Task 1 only after tests and review pass.

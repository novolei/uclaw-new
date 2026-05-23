# PR-4 Soft Interrupts and Boundary Yields Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add jcode-inspired non-destructive interruption foundations and normalize uClaw paused states into existing `TaskEvent::BoundaryYield` / `Checkpoint` contracts.

**Architecture:** PR-4 is an adapter/foundation PR, not a rewrite of `run_agentic_loop`. It adds a clean-room soft-interrupt queue for future runtime wiring, maps resumable human/browser boundaries to existing runtime events, and keeps cancellation, Tauri commands, provider calls, and conversation persistence unchanged.

**Tech Stack:** Rust, Tokio sync primitives, serde, `uclaw-runtime-contracts`, existing `SessionTask` / rollout bridge modules.

---

## ADR §18 Answers

1. **User-visible outcome:** Paused browser and approval-style agent states stop pretending to be terminal completion in rollout events.
2. **Canonical data flow:** `IntentSpec -> TaskSpec -> TaskEvent -> WorldProjection`; PR-4 only changes `TaskEvent` emission.
3. **State owner:** Existing task/session stores remain owners. Soft interrupts are in-memory queue primitives until later control-plane wiring.
4. **Rollback:** Revert the new queue module and event mapping changes; no schema or persisted payload migration.
5. **Policy boundary:** No bypass of `SafetyManager`, permissions, or browser intervention policy.
6. **Human boundary:** `BoundaryYield` is emitted for approval/intervention/checkpoint pause points; terminal `TaskFinished` is withheld when work is resumable.
7. **Failure mode:** Existing `TaskVerdict::Failed` and `TaskVerdict::Cancelled` mappings remain unchanged.
8. **Performance:** Queue operations are O(1) push and O(n) drain over pending interrupts; rollout mapping remains one pass over steps.
9. **Compatibility:** No `TaskEvent` enum variant changes; existing serialized contract shape stays stable.
10. **Verification:** Focused cargo tests for agent interrupt queue, regular task yield mapping, browser rollout mapping, and existing runtime contract tests.
11. **Close loop:** Update `AGENT_OS_JCODE_UPGRADE_STATUS.md` with PR-3 merged, PR-4 branch/worktree, impact findings, verification, and next handoff.

## File Structure

- Create `src-tauri/src/agent/interrupts.rs`: clean-room soft-interrupt queue types and helpers.
- Create `src-tauri/src/agent/interrupts_tests.rs`: sibling tests for queue ordering, urgent tracking, drain, clear, and no cancellation semantics.
- Modify `src-tauri/src/agent/mod.rs`: expose the new `interrupts` module.
- Modify `src-tauri/src/agent/regular_task.rs`: emit `BoundaryYield` and no `TaskFinished` for `LoopOutcome::NeedApproval`; add sibling test module only.
- Create `src-tauri/src/agent/regular_task_pr4_tests.rs`: sibling tests for resumable approval boundary behavior.
- Modify `src-tauri/src/browser/rollout_bridge.rs`: map `NeedsUserIntervention` and `PausedCheckpointed` to `BoundaryYield`; emit `Checkpoint` for browser checkpoint pauses; update existing tests.
- Modify `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: PR-4 closed-loop state.
- Create this plan file.

## Impact Summary

- `run_agentic_loop`: HIGH, not edited in PR-4.
- `outcome_to_verdict`: MEDIUM, direct callers are existing regular-task tests and `run_with_rollout`.
- `browser_run_to_events`: MEDIUM, direct caller is `emit_browser_run_into_session_dir` plus local tests.
- `TaskEvent`: semantically high serialized contract, not edited in PR-4.

## Task 1: Soft Interrupt Queue Foundation

**Files:**
- Create: `src-tauri/src/agent/interrupts.rs`
- Create: `src-tauri/src/agent/interrupts_tests.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: Add sibling tests first**

```rust
use super::*;

#[test]
fn queue_drains_in_fifo_order_and_counts_urgent_messages() {
    let queue = SoftInterruptQueue::default();
    queue.push(SoftInterruptMessage::user("first"));
    queue.push(SoftInterruptMessage::urgent_system("second"));

    let drained = queue.drain();

    assert_eq!(drained.messages.len(), 2);
    assert_eq!(drained.messages[0].content, "first");
    assert_eq!(drained.messages[1].content, "second");
    assert_eq!(drained.urgent_count, 1);
    assert!(queue.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail before implementation**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::interrupts`

Expected: compile failure because `agent::interrupts` is not defined yet.

- [ ] **Step 3: Implement the queue**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftInterruptSource {
    User,
    System,
    Automation,
    BackgroundTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftInterruptMessage {
    pub source: SoftInterruptSource,
    pub content: String,
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftInterruptDrain {
    pub messages: Vec<SoftInterruptMessage>,
    pub urgent_count: usize,
    pub total_content_bytes: usize,
}
```

Queue behavior: `push` appends, `drain` empties exactly once, `clear` returns removed count, `snapshot` preserves pending order, and no method touches `CancellationToken`.

- [ ] **Step 4: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::interrupts`

Expected: all `agent::interrupts` tests pass.

## Task 2: RegularTask Approval Boundary Mapping

**Files:**
- Modify: `src-tauri/src/agent/regular_task.rs`
- Create: `src-tauri/src/agent/regular_task_pr4_tests.rs`

- [ ] **Step 1: Add sibling test**

Test a synthetic delegate returning `LoopOutcome::NeedApproval` and assert:

- events contain `TaskStarted`;
- events contain `BoundaryYield { reason: "awaiting approval for tool `shell`" }`;
- events do not contain `TaskFinished`.

- [ ] **Step 2: Implement minimal mapping**

Add a helper:

```rust
fn outcome_to_boundary_yield_reason(outcome: &LoopOutcome) -> Option<String> {
    match outcome {
        LoopOutcome::NeedApproval { tool_name, tool_call_id, .. } => {
            Some(format!("awaiting approval for tool `{tool_name}` ({tool_call_id})"))
        }
        _ => None,
    }
}
```

After intermediate events, push `TaskEvent::BoundaryYield` and return early when this helper returns `Some`.

- [ ] **Step 3: Preserve verdict mapping**

Keep `outcome_to_verdict` unchanged for compatibility; `run_with_rollout` can still use it until later PRs migrate that adapter.

- [ ] **Step 4: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::regular_task`

Expected: existing regular task tests and PR-4 sibling tests pass.

## Task 3: Browser Rollout Boundary Mapping

**Files:**
- Modify: `src-tauri/src/browser/rollout_bridge.rs`

- [ ] **Step 1: Update tests**

Change `user_intervention_emits_permission_pair` so it expects:

- `PermissionRequested`;
- `PermissionDecided`;
- `BoundaryYield`;
- no `TaskFinished`.

Add or update a checkpoint test expecting:

- `Checkpoint { checkpoint_ref: "browser:<run_id>:paused_checkpointed" }`;
- `BoundaryYield { reason: "browser checkpoint paused" }`;
- no `TaskFinished`.

- [ ] **Step 2: Implement mapping**

For `BrowserTaskStatus::NeedsUserIntervention`, emit `BoundaryYield` and no terminal verdict.

For `BrowserTaskStatus::PausedCheckpointed`, emit `Checkpoint`, then `BoundaryYield`, and no terminal verdict.

- [ ] **Step 3: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`

Expected: browser rollout bridge tests pass.

## Task 4: Status, Verification, and Close Loop

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [ ] **Step 1: Update PR ledger**

Mark PR-3 merged at GitHub PR #401 / merge commit `9af769c1`; mark PR-4 in progress on branch `codex/agent-os-jcode-pr4-soft-interrupts`.

- [ ] **Step 2: Run verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib agent::interrupts
cargo test --manifest-path src-tauri/Cargo.toml --lib agent::regular_task
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test -p uclaw-runtime-contracts -- contracts_tests
git diff --check -- docs/superpowers/plans/2026-05-23-pr4-soft-interrupts-boundary-yields.md docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md src-tauri/src/agent/mod.rs src-tauri/src/agent/interrupts.rs src-tauri/src/agent/interrupts_tests.rs src-tauri/src/agent/regular_task.rs src-tauri/src/agent/regular_task_pr4_tests.rs src-tauri/src/browser/rollout_bridge.rs
npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr4-soft-interrupts
```

Expected: focused tests pass, diff check passes, GitNexus detects only the PR-4 files and reports no unexpected high-risk runtime rewrite.

## Self-Review

- Spec coverage: soft interrupt foundation, boundary yield semantics, browser checkpoint/intervention mapping, and closed-loop status are covered.
- Placeholder scan: no TBD/TODO instructions; every task has files, exact behavior, commands, and expected output.
- Type consistency: uses existing `TaskEvent::BoundaryYield`, `TaskEvent::Checkpoint`, `PermissionRequested`, `PermissionDecided`, and `LoopOutcome::NeedApproval` variants without modifying serialized contracts.

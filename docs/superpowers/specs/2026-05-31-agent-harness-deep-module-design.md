# AgentHarness Deep Module Design

**Date:** 2026-05-31
**Status:** First run lifecycle slice implemented
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi/packages/agent/src/harness/agent-harness.ts`, `/Users/ryanliu/Documents/pi/packages/coding-agent/src/core/agent-session.ts`, `/Users/ryanliu/Documents/pi_agent_rust/src/agent.rs`

## Problem

`AgentHarness` is currently a pass-through into `run_assembly::run_agent`.
That makes the module shallow: callers still need to know low-level
`run_agentic_loop` setup details such as `ReasoningContext.force_text`,
cancellation-token installation, timeout, task hook dispatch, and direct
low-level loop ownership.

The current production direct low-level callers are:

- `src-tauri/src/agent/regular_task.rs`
- `src-tauri/src/agent/rollout_integration.rs`
- `src-tauri/src/agent/teams/worker.rs`

`agentic_loop.rs` tests may continue to call `run_agentic_loop` directly because
they test the low-level Implementation.

## Goal

Make `AgentHarness` the Deep Module for run/session lifecycle execution while
leaving `agentic_loop` as the low-level Implementation.

The first slice deepens only the run lifecycle seam. Later slices can add
session persistence, compaction, queued input, and resource ownership after the
Interface is earning its keep.

## Design

### Module shape

`src-tauri/src/agent/harness.rs` becomes the public run lifecycle Interface.
It should own:

- start-of-run context preparation (`force_text = false`);
- cancellation token installation into `ReasoningContext`;
- timeout/cancellation selection;
- `TaskStart` and `TaskEnd` hook events;
- eventual room for rollout/session adapters.

`src-tauri/src/agent/run_assembly.rs` can either become an internal helper or
be folded into `harness.rs`. The important seam is that production callers call
the harness, not `run_agentic_loop`.

### First slice Interface

The first slice can keep the existing function entrypoint for compatibility:

```rust
pub async fn run_agent_harness(
    delegate: &dyn LoopDelegate,
    ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    token: CancellationToken,
    hook_bus: Arc<HookBus>,
    run_config: AgentHarnessRunConfig,
) -> AgentHarnessRunOutcome
```

But the Implementation behind it must:

1. Reset `ctx.force_text = false`.
2. Install `ctx.cancellation_token = Some(token.clone())`.
3. Dispatch `TaskStart`.
4. Run `run_agentic_loop` under timeout/cancellation.
5. Dispatch `TaskEnd`.

This preserves the current Interface while increasing depth. A later slice can
replace the function with an `AgentHarness` struct once callsites no longer
depend on direct low-level details.

## Migration Targets

### `RegularTask::run`

Replace its local force-text reset, cancellation-token installation, and direct
`run_agentic_loop` call with `run_agent_harness`. Keep rollout-specific
`TaskEvent` emission outside the harness until the rollout adapter is designed.

### `run_with_rollout`

This function has a public helper role and already emits rollout events. In the
first slice it should either call the harness with an explicit token/hook bus
from callers or remain documented as a low-level adapter pending the rollout
adapter child slice. It must not duplicate force-text/cancellation facts without
a comment explaining why.

### `teams/worker.rs`

The worker direct call is a production callsite. It should move to harness if a
`HookBus` is available. If one is not available in the current worker context,
the child plan must either thread it in or explicitly defer this with evidence.

## Acceptance Evidence

- A failing test first proves harness-managed runs reset `force_text` before
  the loop observes the context.
- A failing test first proves harness-managed runs install the cancellation
  token before the loop observes the context.
- A failing test first proves `TaskStart` and `TaskEnd` dispatch exactly once on
  completed runs.
- A failing test first proves cancelled runs dispatch `TaskEnd` with
  `"cancelled"`.
- `RegularTask::run` no longer directly calls `run_agentic_loop`.
- `rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime`
  shows only low-level Implementation/tests and explicitly justified adapters.
- Focused tests pass:

```bash
cd src-tauri && cargo test --lib agent::harness
cd src-tauri && cargo test --lib agent::run_assembly
cd src-tauri && cargo test --lib agent::regular_task
```

## Implementation Evidence

- Commit `ec7ca99e` deepens `run_agent_harness` with context preparation and
  migrates `RegularTask::run` through the harness seam.
- Focused tests passed: `agent::harness` 2 passed, `agent::run_assembly` 2
  passed, and `agent::regular_task` 12 passed.
- Search verification showed `force_text` reset and cancellation-token
  installation now live only in `src-tauri/src/agent/harness.rs`.
- Local code review found no Critical or Important findings. The remaining
  direct production adapters are documented as deferred low-level rollout/team
  adapters for this first slice.

## Risks

- `RegularTask::run` currently emits rich rollout `TaskEvent`s after the loop.
  The harness slice must not remove those events.
- `HookBus` is observe-only for `TaskStart`/`TaskEnd`; tests must avoid
  asserting decisions.
- Timeouts and cancellation can race in async tests. Keep the cancellation test
  deterministic by cancelling before calling the harness, or by using a delegate
  that awaits until cancelled.

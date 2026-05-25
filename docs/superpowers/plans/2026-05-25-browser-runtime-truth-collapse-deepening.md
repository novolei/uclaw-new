# Browser Runtime Truth Collapse Deepening

## Intent

Deepen the Browser Runtime Module so browser task execution no longer has to
know the full ordering of runtime status inspection, provider route selection,
rollout signal emission, and provider execution. This implements the
`Collapse Browser Runtime truth` candidate from
`/private/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/architecture-review-20260525-192243.html`
as a narrow architecture slice.

This is not a new Browser Runtime phase train. It is a follow-up Module
deepening slice on top of the already merged Browser Runtime Supervisor /
Playwright Provider work.

## ADR 18 Check

| Question | Answer |
|---|---|
| 1. Intent | Concentrate browser runtime action truth behind one small Interface for task-time browser actions. |
| 2. Autonomy | Browser task autonomy keeps the same observe/decide/act loop; only the runtime execution seam changes. |
| 3. Truth source | `BrowserRuntimeStatusService` remains the source for aggregate Rust runtime status. The new Module consumes it internally instead of leaking route options to callers. |
| 4. TaskEvent | Provider route `Signal` events stay emitted through rollout bridge helpers, but emission moves behind the runtime execution Interface. |
| 5. Context | No prompt or Context Fabric changes. |
| 6. Capability | Existing `BrowserProviderActionExecutor` and provider route policy are reused as internal implementation. |
| 7. Hooks | No new policy hooks; existing provider and feature-flag gates remain in force. |
| 8. Projection | No frontend projection change; this slice preserves the current status and projection read models. |
| 9. Harness | Focused Rust tests cover the new Interface plus existing provider execution and agent loop regressions. |
| 10. Rollback | Revert the new Module, `browser/mod.rs` export, and `BrowserAgentLoop` wiring. Existing provider execution remains intact. |
| 11. Non-ownership | No provider default promotion, no hosted provider, no runtime-pack mutation, no Settings action execution, no UI work, no DB migration, and no new TaskEvent schema. |

## Module Shape

### Before

`BrowserAgentLoop` has to know this sequence:

1. Inspect `BrowserRuntimeStatusService`.
2. Convert the report into `BrowserProviderActionRouteOptions`.
3. Construct `BrowserProviderActionExecutor`.
4. Route the action.
5. Emit provider route signals into rollout.
6. Execute the routed action with identity.
7. Interpret the execution outcome into task steps/checkpoints.

That makes the Module shallow: the caller crosses several Interfaces and must
remember the ordering.

### After

Add `src-tauri/src/browser/runtime_execution.rs`.

The external Interface is a small `BrowserRuntimeActionExecutor` with one
task-time action method. It accepts session id, optional identity profile id,
task id, and `BrowserAction`; internally it:

- inspects the runtime status when the status service is available;
- derives provider route options;
- routes the action through existing provider policy;
- emits rollout-visible provider route signals;
- executes the routed action through the existing provider adapter;
- returns the existing provider action outcome.

The existing `BrowserProviderActionExecutor` stays as the lower-level Adapter.
The new Browser Runtime action Module is the deeper seam that task callers use.

## Allowed Files

- `src-tauri/src/browser/runtime_execution.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan

## GitNexus Impact

Fresh index:

```text
npx gitnexus analyze
Expected: repository indexed successfully for this worktree.
Observed: 39,328 nodes, 65,524 edges, 300 flows.
```

Pre-edit impact:

- `BrowserAgentLoop` impl: LOW, 0 direct callers, 0 affected processes.
- `BrowserProviderActionExecutor` impl: LOW, 0 direct callers, 0 affected processes.
- `BrowserRuntimeStatusService` struct: LOW, 0 direct callers, 0 affected processes.

## Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_execution.rs src-tauri/src/browser/agent_loop.rs
rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/browser/mod.rs
git diff --check -- src-tauri/src/browser/runtime_execution.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-truth-collapse-deepening.md
```

Expected:

- new runtime execution tests pass;
- existing browser task loop and provider execution tests pass;
- formatting and whitespace checks pass;
- GitNexus detect-changes reports no unexpected HIGH/CRITICAL risk.

Observed:

- `browser::runtime_execution`: `2 passed`.
- `browser::agent_loop`: `14 passed`.
- `browser::provider_execution`: `8 passed`.
- `rustfmt` checks passed for `runtime_execution.rs`, `agent_loop.rs`, and
  `mod.rs` with `skip_children=true` for the module file.
- `git diff --check` passed.
- GitNexus staged detect reported LOW with `changed_files: 5`,
  `affected_count: 0`, and no affected processes.

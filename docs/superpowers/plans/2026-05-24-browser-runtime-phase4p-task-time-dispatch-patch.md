# Phase 4P - Task-Time Dispatch Patch Boundary

## Summary

Phase 4P implements the accepted low-risk writer path from Phase 4O: keep
prompt-dispatch wiring inside `dispatcher.rs` and add a small pure boundary
that normalizes task-time Browser runtime prompt patches before tool approval
and execution.

This phase does not display the prompt, run runtime-pack operations, add IPC, or
edit `agentic_loop.rs`. It only makes the backend dispatcher ready to consume a
serialized Browser task request patch in a single tested place.

## ADR Section 18 Questions

1. What user intent does this support?
   Users who defer Browser runtime preparation at task time need that decision
   to reach `browser_task` execution as `runtime_preparation_decision: "defer"`
   so the existing pause checkpoint can run.
2. What autonomy level can it run at?
   The boundary is automatic normalization of already-supplied tool-call
   arguments. It does not initiate preparation, network access, deletion, or
   provider changes.
3. What is the canonical truth source?
   The canonical truth remains the `browser_task` tool arguments, browser task
   run/checkpoint state, and emitted browser task events. Dispatcher
   normalization is a transport boundary only.
4. What TaskEvent entries does it emit?
   None directly. Existing `browser_task` execution remains responsible for
   checkpoint/run events when `runtime_preparation_decision` is `defer`.
5. What context does it read, and how is it cited?
   It reads only the current `ToolCall` name and JSON arguments already emitted
   by the model/UI bridge. No external context is fetched.
6. What capability cards does it add or consume?
   None. It consumes the existing `browser_task` tool contract.
7. What policy hooks can block it?
   Existing tool approval, path policy, safety mode, and browser task execution
   errors continue to block execution. This phase does not weaken approval.
8. What world projection does the UI render?
   No UI changes. Existing task-time prompt and paused-waiting projection remain
   the visible surfaces.
9. What harness cases prove it works?
   Focused dispatcher unit tests prove patch normalization applies only to
   `browser_task`, preserves explicit runtime decisions, and leaves non-browser
   tools unchanged. Existing browser runtime regressions prove no runtime-pack or
   provider contracts changed.
10. What is the rollback or disable path?
   Revert the single PR. Without this boundary, callers can still pass the
   already-flat `runtime_preparation_decision` argument directly to
   `browser_task`.
11. What does it deliberately not own?
   It does not own prompt display, ApprovalModal changes, Settings IPC, runtime
   prepare/repair/install/cleanup/rollback execution, no-browser fallback
   execution, provider promotion, `agentic_loop.rs`, `tauri_commands.rs`, DB
   migrations, root `App`, or global npm/Playwright paths.

## Allowed Files

- `src-tauri/src/agent/dispatcher.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4p-task-time-dispatch-patch.md`

## Non-Goals

- Do not edit `agentic_loop.rs`, `tauri_commands.rs`, `CLAUDE.md`,
  `BEHAVIOR.md`, DB migrations, workspace `Cargo.toml`, or root `App`.
- Do not add UI/IPC wiring or mutate approval response schemas.
- Do not execute runtime-pack operations or launch Playwright.
- Do not implement no-browser fallback selection.
- Do not promote Playwright CLI/MCP/hosted providers.

## Impact Targets

- `ChatDelegate.execute_tool_calls` in `src-tauri/src/agent/dispatcher.rs`:
  GitNexus impact LOW, 0 direct callers, 0 affected processes, 0 affected
  modules.

## Implementation Notes

- Normalize only `browser_task` arguments.
- Accept `browser_task_request_patch` or `browserTaskRequestPatch` wrapper
  objects containing `runtime_preparation_decision`.
- Do not override an explicit top-level `runtime_preparation_decision`.
- Strip the wrapper field from the executed/approval arguments so the
  `browser_task` tool receives its stable schema.

## Rollback

Revert the Phase 4P PR. Runtime behavior falls back to the existing flat
`runtime_preparation_decision` argument supported by `BrowserTaskTool`.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::dispatcher`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/agent/dispatcher.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

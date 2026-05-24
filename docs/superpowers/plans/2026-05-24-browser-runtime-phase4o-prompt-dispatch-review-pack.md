# Phase 4O - Prompt Dispatch Review Pack

## Summary

Phase 4N added a pure helper for applying task-time runtime defer choices to
serialized `browser_task` arguments. The next true behavior slice is prompt
dispatch / tool-call wiring, which sits near agent approval and loop hot paths.

Phase 4O is a deliberately docs-only reviewer pack. It records the GitNexus
blast radius, defines the only acceptable writer scopes for the next PR, and
blocks any implementation that would touch the DMZ `agentic_loop.rs` without a
fresh reviewer acceptance.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need task-time Browser runtime defer decisions to reach the backend
   pause gate without destabilizing normal agent tool execution.
2. What autonomy level can it run at?
   Docs/reviewer planning only. It executes no tools and changes no runtime
   behavior.
3. What is the canonical truth source?
   The canonical truth remains browser task runs, task status, checkpoints,
   and TaskEvents. This pack only records the acceptance criteria for wiring
   that truth safely.
4. What TaskEvent entries does it emit?
   None. Future implementation must preserve the existing browser task
   checkpoint and boundary events.
5. What context does it read, and how is it cited?
   It reads the ADR, tracker, GitNexus impact results, and the current
   dispatcher/browser-tool boundaries. The impact findings are copied into this
   plan and tracker.
6. What capability cards does it add or consume?
   None. Future implementation will consume the existing browser task/runtime
   capability surfaces.
7. What policy hooks can block it?
   Tool approval, runtime-pack policy, metered/offline download confirmation,
   and DMZ reviewer acceptance can block the future implementation.
8. What world projection does the UI render?
   No UI change in this slice. Future implementation must preserve the
   task-time prompt, Browser Runtime settings, and paused-waiting task
   projection already merged in Phase 4D-4N.
9. What harness cases prove it works?
   This docs-only pack is verified by GitNexus impact/detect and diff checks.
   The next implementation must run focused dispatcher/browser-task tests plus
   the default browser runtime regressions.
10. What is the rollback or disable path?
   Revert this docs PR. It changes no runtime state, migrations, settings,
   provider state, or user data.
11. What does it deliberately not own?
   It does not implement prompt dispatch, mutate tool calls, change approval
   decisions, run runtime-pack actions, execute no-browser fallback, promote
   providers, edit DMZ files, or touch DB migrations.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4o-prompt-dispatch-review-pack.md`

## Non-Goals

- Do not edit `agentic_loop.rs`, `tauri_commands.rs`, root `App`, Settings
  IPC, DB migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not edit `dispatcher.rs`, browser tools, approval code, or frontend
  prompt code in this phase.
- Do not execute runtime-pack prepare/repair/install/cleanup/rollback.
- Do not wire prompt dispatch or mutate tool-call arguments.

## Impact Targets

- `ChatDelegate.execute_tool_calls` in `src-tauri/src/agent/dispatcher.rs`:
  GitNexus impact LOW, 0 direct callers, 0 affected processes.
- `run_agentic_loop` in `src-tauri/src/agent/agentic_loop.rs`: GitNexus impact
  HIGH, 4 direct callers, 7 impacted symbols, affected modules Agent,
  Channels, and Runtime. This file is DMZ and must not be edited without
  reviewer acceptance.
- `BrowserTaskTool.execute` in `src-tauri/src/browser/tools.rs`: GitNexus
  impact LOW, 0 direct callers, 0 affected processes.

## Writer Plan For Next Implementation

The next implementation PR may proceed only if it stays within one of these
writer scopes:

- Preferred: `dispatcher.rs`-only utility/wiring that applies the Phase 4N
  helper-equivalent decision patch before approval/execution, with focused
  dispatcher tests and no changes to `agentic_loop.rs`.
- Alternate: browser-tool-level prompt/decision handling inside
  `BrowserTaskTool.execute`, with focused `browser::tools` and
  `browser::agent_loop` regressions, and no changes to global agent loop
  control flow.

Any proposal that edits `agentic_loop.rs`, `tauri_commands.rs`, DB migrations,
or root `App` must stop and spawn a fresh reviewer sub-agent before
implementation.

## Reviewer Checklist

- Confirm the writer scope avoids `agentic_loop.rs` unless the reviewer
  explicitly accepts the HIGH/DMZ blast radius.
- Confirm approval semantics are preserved: no tool executes without the same
  or stricter approval/policy checks.
- Confirm no-browser fallback remains explicit and does not silently skip a
  browser-required task.
- Confirm runtime-pack prepare/repair/cleanup/rollback side effects are not
  introduced in the dispatch PR.
- Confirm rollback is a single-PR revert.

## Rollback

Revert the Phase 4O docs PR. It has no code or persistent side effects.

## Verification

- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
- Default browser runtime regressions remain recommended for this program, but
  this docs-only phase does not change Rust or UI code.

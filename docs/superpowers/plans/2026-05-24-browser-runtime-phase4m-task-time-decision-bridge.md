# Phase 4M - Task-Time Decision Bridge

## Summary

Phase 4L added the backend `browser_task` defer gate. Phase 4M adds the
frontend/model bridge that makes the task-time prompt action produce a
backend-ready `runtime_preparation_decision` payload when deferral must pause
the browser task.

This is intentionally not prompt dispatch. The PR only makes the existing
prompt model carry typed decision metadata that a later IPC/dispatcher slice can
consume.

## ADR Section 18 Questions

1. What user intent does this support?
   Users who choose "defer" on a task-time runtime prompt need that choice to
   map cleanly to the backend pause gate added in Phase 4L.
2. What autonomy level can it run at?
   Read/model only. The derived action metadata does not execute tools, mutate
   runtime files, or approve browser automation.
3. What is the canonical truth source?
   The prompt model remains a UI projection. The canonical backend behavior is
   still `BrowserTaskRequest.runtime_preparation_decision` and
   `BrowserTaskRun.status`.
4. What TaskEvent entries does it emit?
   None in this slice. It preserves existing event-name previews and adds only
   typed action payload metadata for future dispatch.
5. What context does it read, and how is it cited?
   It reads the existing runtime status report inputs to
   `deriveBrowserRuntimeTaskTimePrompt`; no external context is read.
6. What capability cards does it add or consume?
   None. It consumes the existing Browser runtime task-time prompt model.
7. What policy hooks can block it?
   None in this slice because it has no side effects. Policy and confirmation
   gates remain in future runtime-pack execution and prompt-dispatch phases.
8. What world projection does the UI render?
   Existing prompt UI remains unchanged. The model carries a backend-ready
   decision payload for the checkpointed defer action.
9. What harness cases prove it works?
   Focused Vitest model coverage proves checkpointed defer actions carry
   `runtime_preparation_decision: "defer"` and no-browser fallback actions do
   not accidentally request a browser pause.
10. What is the rollback or disable path?
   Revert this PR. Without the metadata, the prompt still renders as before,
   and backend behavior remains available through the explicit Phase 4L field.
11. What does it deliberately not own?
   It does not own prompt dispatch, Tauri IPC, tool approval mutation,
   Settings persistence, runtime-pack execution, no-browser fallback execution,
   provider promotion, Playwright launch, DB migrations, or DMZ edits.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4m-task-time-decision-bridge.md`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`

## Non-Goals

- Do not edit `tauri_commands.rs`, `agentic_loop.rs`, root `App`, Settings
  IPC, DB migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not execute runtime-pack prepare/repair/install/cleanup/rollback.
- Do not approve, rewrite, or dispatch tool calls.
- Do not implement no-browser fallback execution.
- Do not change prompt rendering or layout.

## Impact Targets

- `deriveBrowserRuntimeTaskTimePrompt` in
  `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`.
- `BrowserRuntimeTaskTimePromptAction` in
  `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`.
- `BrowserRuntimeTaskTimePrompt` in
  `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.tsx` as the
  direct consumer of the action interface.

GitNexus impact for all three targets reported LOW risk before edits. The
action interface has one direct importer and 0 affected processes.

## Rollback

Revert the Phase 4M PR. No migrations, persistent settings, runtime files,
provider state, or task records are changed.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- Browser runtime default Rust regressions:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

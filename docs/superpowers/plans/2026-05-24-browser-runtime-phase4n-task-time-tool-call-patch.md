# Phase 4N - Task-Time Tool-Call Patch Boundary

## Summary

Phase 4M made task-time prompt actions carry a backend-ready
`runtime_preparation_decision` payload. Phase 4N adds the next narrow boundary:
a pure, tested helper that applies that payload to serialized `browser_task`
arguments without wiring it into prompt dispatch, approval, IPC, or backend
execution.

This keeps the future dispatch PR small and auditable: it will consume a
single helper instead of re-deriving runtime-defer semantics inside a hot path.

## ADR Section 18 Questions

1. What user intent does this support?
   Users who defer Browser runtime preparation at task time need that decision
   to become the exact backend `browser_task` argument that pauses the task.
2. What autonomy level can it run at?
   Pure model/argument transformation only. It has no side effects and does
   not execute, approve, or dispatch tools.
3. What is the canonical truth source?
   The canonical backend behavior remains
   `BrowserTaskRequest.runtime_preparation_decision` and
   `BrowserTaskRun.status`. This helper only prepares serialized arguments for
   a future dispatcher slice.
4. What TaskEvent entries does it emit?
   None. Event emission remains owned by backend browser task execution and
   rollout bridge code.
5. What context does it read, and how is it cited?
   It reads only the prompt action and caller-supplied tool name/arguments.
   There is no external context or citation requirement in this slice.
6. What capability cards does it add or consume?
   None. It consumes the existing browser task-time prompt action contract.
7. What policy hooks can block it?
   None in this slice because the helper does not execute a tool. Tool
   approval, runtime-pack policy, and download/destructive confirmations remain
   future gates.
8. What world projection does the UI render?
   No rendering changes. The helper preserves the already-rendered prompt
   decision and translates it into backend-ready `browser_task` arguments.
9. What harness cases prove it works?
   Focused Vitest coverage proves checkpointed defer patches only
   `browser_task` arguments with `runtime_preparation_decision: "defer"`,
   leaves no-browser fallback actions unpatched, preserves existing arguments,
   and ignores non-browser tools.
10. What is the rollback or disable path?
   Revert this PR. Prompt rendering and backend explicit defer behavior remain
   unchanged.
11. What does it deliberately not own?
   It does not own prompt dispatch, Tauri IPC, tool approval mutation, backend
   browser task execution, runtime-pack execution, Settings actions,
   no-browser fallback execution, provider promotion, DB migrations, or DMZ
   edits.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4n-task-time-tool-call-patch.md`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`

## Non-Goals

- Do not edit `tauri_commands.rs`, `agentic_loop.rs`, root `App`, Settings
  IPC, DB migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not wire the helper into prompt dispatch or approval paths.
- Do not execute runtime-pack prepare/repair/install/cleanup/rollback.
- Do not implement no-browser fallback execution.
- Do not change prompt rendering or layout.

## Impact Targets

- `deriveBrowserRuntimeTaskTimePrompt` in
  `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`.
- `browserTaskRuntimeDecisionPayloadForAction` in
  `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`.

GitNexus impact for both targets reported LOW risk with 0 affected processes
before edits in the Phase 4N worktree. `BrowserRuntimeTaskTimePromptAction`
was not resolvable as an indexed symbol in this worktree, so no existing symbol
edit is planned for that interface.

## Rollback

Revert the Phase 4N PR. No migrations, settings, runtime files, provider
state, task records, or browser sessions are changed.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- Browser runtime default Rust regressions:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>`; expected not
  applicable because no Rust files should change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

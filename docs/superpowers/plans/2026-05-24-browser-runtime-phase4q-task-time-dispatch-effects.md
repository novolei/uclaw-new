# Phase 4Q - Task-Time Dispatch Effects

## Summary

Phase 4Q adds a pure frontend dispatch-effect model for task-time Browser
runtime prompt actions. Phase 4M-4N defined the defer payload, Phase 4P taught
the Rust dispatcher how to consume serialized Browser task request patches, and
this phase gives every prompt button an explicit typed effect.

This does not wire UI state into the live agent loop, execute runtime-pack
operations, or implement no-browser fallback execution. It makes the next
integration PR smaller by giving `prepare_now`, `defer`, and
`continue_without_browser` one tested model boundary.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need each task-time Browser runtime prompt action to have an explicit
   dispatch meaning before it is wired to live tool calls.
2. What autonomy level can it run at?
   Pure frontend modeling only. No tool calls, runtime preparation, network,
   filesystem mutation, or provider selection occurs.
3. What is the canonical truth source?
   The prompt action view model remains the source for user choice metadata;
   backend truth remains `browser_task` arguments and browser task run state
   after later integration.
4. What TaskEvent entries does it emit?
   None directly. The dispatch effects carry intended event names for later
   integration.
5. What context does it read, and how is it cited?
   It reads only the selected prompt action and the target tool name supplied
   by the caller.
6. What capability cards does it add or consume?
   None. It consumes the existing Browser runtime prompt and `browser_task`
   contract.
7. What policy hooks can block it?
   Existing future hooks remain tool approval, runtime-pack policy, and task
   fallback policy. This phase does not execute those hooks.
8. What world projection does the UI render?
   No visual change. Existing prompt UI rendering remains unchanged.
9. What harness cases prove it works?
   Focused prompt-model tests prove prepare-now, checkpointed defer,
   recorded defer, no-browser fallback, and non-browser tools map to distinct
   dispatch effects.
10. What is the rollback or disable path?
   Revert the single frontend-model PR. Existing prompt rendering and
   `browser_task` defer patch helpers remain intact.
11. What does it deliberately not own?
   It does not wire ApprovalModal, Settings IPC, `agentic_loop.rs`, backend
   tool execution, no-browser fallback execution, runtime-pack
   prepare/repair/install/cleanup/rollback, provider promotion, or DB
   migrations.

## Allowed Files

- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4q-task-time-dispatch-effects.md`

## Non-Goals

- Do not edit `agentic_loop.rs`, `tauri_commands.rs`, root `App`, DB
  migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not add IPC commands or ApprovalModal schemas.
- Do not execute runtime-pack operations or launch Playwright.
- Do not implement no-browser fallback execution.
- Do not change rendered prompt UI.

## Impact Targets

- This phase adds exported type/function helpers and tests in the existing
  frontend prompt model. No existing function/class/method body is modified.

## Rollback

Revert the Phase 4Q PR. Existing prompt actions and backend defer behavior
continue to work as before.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` (expected N/A)
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

# Phase 5C - Playwright CLI Worker Script Contract

## Summary

Add the app-managed Playwright CLI worker script that the Phase 5B Rust runner
executes. This phase proves the v1 declarative action protocol can be handled by
a runtime-pack worker asset with local fixture coverage, while keeping provider
promotion, task routing, IPC, and UI out of scope.

## ADR 18 Questions

1. **User intent:** run selected local browser actions through a faster
   Playwright CLI lane once the app-managed runtime pack is ready.
2. **Autonomy level:** L1-L3 only; this PR adds an internal worker contract and
   fixture tests, not a new user-facing provider default.
3. **Canonical truth:** uClaw Rust task/run/artifact state remains canonical.
   The worker emits one structured result envelope that Rust validates.
4. **TaskEvents:** none emitted in this slice. Future routing will translate
   worker results into browser TaskEvents.
5. **Context read/cited:** consumes the Phase 5A request envelope, Phase 5B
   child-worker runner, ADR section 7.1, and current Playwright docs for
   locator/action APIs.
6. **Capability cards:** consumes the existing `browser.playwright_cli`
   capability card; no new provider card.
7. **Policy hooks:** raw script/evaluate remains absent; actions are
   declarative, timeout-bounded, and run from the app-managed pack path.
8. **World projection:** no UI projection change. Future routing will project
   action, artifact, and recovery events.
9. **Harness cases:** Rust fixture tests run the real worker script against a
   fake local Playwright module and cover success/failure envelopes for bounded
   declarative actions.
10. **Rollback/disable:** revert this PR. The CLI provider remains
    feature-flagged and unpromoted.
11. **Non-ownership:** no task routing, no Tauri IPC, no Settings UI, no DB
    migration, no `agentic_loop.rs`, no `tauri_commands.rs`, no global npm, and
    no user-installed Playwright production path.

## Allowed Files

- `src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`
- `src-tauri/src/browser/playwright_cli.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5c-cli-worker-script.md`

## Non-Goals

- Do not promote `browser.playwright_cli`.
- Do not route browser tasks through Playwright CLI.
- Do not add IPC, Settings, Startup Doctor, DB migration, or artifact storage.
- Do not add arbitrary raw Playwright script execution.
- Do not require global npm or global Playwright packages for production.

## Impact Targets

- GitNexus impact for `run_playwright_cli_child_worker`: MEDIUM, 5 direct test
  callers, 0 affected processes.
- GitNexus impact for `PlaywrightCliRequestEnvelope`: LOW, 2 direct callers, 0
  affected processes.
- New worker asset has no existing callers before this PR.

## Implementation Steps

1. Add a standalone ESM worker script under `src-tauri/resources/browser-runtime/worker/`.
2. Implement stdin JSON parsing, request validation, declarative action dispatch,
   structured success/failure envelopes, and browser/context cleanup.
3. Use Playwright APIs for `navigate`, `click`, `type`, `screenshot`,
   `extract`, and `wait`, with addressing order matching the ADR.
4. Add Rust fixture tests that copy the worker into a temp app-managed pack and
   provide a fake local `playwright` module, avoiding network/browser downloads.
5. Update the tracker with phase status, verification, and next action.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`
- `node --check src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect-changes`

## Rollback

Revert this PR. Phase 5B's Rust runner still exists but will keep using whatever
worker script is present in the managed pack; no production task route depends
on this worker yet.

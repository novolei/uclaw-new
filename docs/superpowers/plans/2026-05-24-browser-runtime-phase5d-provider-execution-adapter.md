# Phase 5D - Playwright CLI Provider Execution Adapter

## Goal

Add a narrow Playwright CLI provider execution adapter that turns a feature-flagged,
runtime-ready declarative action into one supervised child-worker run. This is
the first callable provider boundary for the CLI lane, but it remains unpromoted:
no agent-loop routing, no Settings IPC, no default provider selection, and no
task-event side effects.

## ADR Section 18 Questions

1. What user intent does this support?
   - Browser automation intents that can be expressed as bounded declarative
     actions: navigate, click, type, screenshot, extract, and wait.
2. What autonomy level can it run at?
   - L1-L3 only for this phase. Higher autonomy, irreversible actions, payments,
     posting, account mutation, and credential flows remain policy-blocked by
     future routing layers.
3. What is the canonical truth source?
   - The uClaw task/run/event model remains canonical. This adapter returns a
     typed execution result; it does not create a second task database or
     provider-owned truth source.
4. What TaskEvent entries does it emit?
   - None in this phase. TaskEvent emission belongs to later task-routing /
     supervisor integration so this PR stays reversible and side-effect free.
5. What context does it read, and how is it cited?
   - It reads feature flags, the runtime-pack status report, the declarative
     action, and the request id. Any model-visible artifact is returned as an
     artifact ref from the worker result.
6. What capability cards does it add or consume?
   - It consumes the existing `browser.playwright_cli` capability/readiness
     contract and the app-managed runtime-pack readiness report. It adds no new
     provider card.
7. What policy hooks can block it?
   - Feature flag disabled, runtime pack not ready, unsupported/raw-script
     action absence, action timeout, worker failure, and future policy gates for
     unsafe browser actions. This phase models the local adapter gates only.
8. What world projection does the UI render?
   - No new UI. The adapter result is shaped so later projection can render
     provider id, action kind, status, retryability, artifacts, and output.
9. What harness cases prove it works?
   - Focused Rust tests cover disabled feature flag, unready runtime, successful
     worker execution, worker structured failure, timeout/nonzero runner error
     mapping, artifact refs, and no raw-script action surface.
10. What is the rollback or disable path?
   - Revert this PR or disable `playwright_cli`; chromiumoxide remains the
     default browser path and Phase 5A-5C contracts remain available.
11. What does it deliberately not own?
   - Agent-loop routing, provider promotion, BrowserProvider parity routing,
     Settings/IPC, DB migrations, identity/profile UX, Playwright MCP, raw
     script execution, and hosted providers.

## Allowed Files

- `src-tauri/src/browser/playwright_cli.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5d-provider-execution-adapter.md`

## Non-Goals

- No edits to `agentic_loop.rs` or `tauri_commands.rs`.
- No provider promotion or task routing.
- No UI, IPC, DB migration, or runtime-pack mutation.
- No global npm or user-installed Playwright production path.
- No arbitrary Playwright script escape hatch.

## Impact Targets

- GitNexus impact for `build_playwright_cli_request_envelope`: LOW, 2 direct
  test callers, 0 affected processes.
- GitNexus impact for `run_playwright_cli_child_worker`: MEDIUM, 7 direct test
  callers, 0 affected processes.
- GitNexus impact for `PlaywrightCliWorkerResultEnvelope`: LOW, 0 affected
  processes.
- GitNexus impact for `PlaywrightCliWorkerError`: LOW, 0 affected processes.

## Implementation Plan

1. Add typed provider execution request/result/error structures.
2. Add a provider adapter function that gates on the feature flag and runtime
   readiness, builds the existing request envelope, invokes the supervised child
   worker, and maps worker failures into structured provider failures.
3. Add focused tests for success, disabled feature flag, runtime-not-ready,
   worker structured failure passthrough, and runner error classification.
4. Update the tracker with Phase 5D entry criteria, progress, impact, and
   verification notes.

## Rollback

Revert the Phase 5D commit. The Phase 5A provider contract, Phase 5B
child-worker runner, and Phase 5C worker script remain intact, but no callable
Playwright CLI provider adapter is exposed.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`
- `git diff --check -- src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5d-provider-execution-adapter.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5d-provider-execution-adapter`

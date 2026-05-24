# Phase 5E - Playwright CLI Fixture Gates

## Goal

Close the remaining ADR Phase 5 fixture evidence for the Playwright CLI thin
lane: locator fallback order, coordinate fallback, risk-based screenshot
artifact behavior, and declarative action outputs. This is a test/verification
slice; it does not change provider routing or production task behavior.

## ADR Section 18 Questions

1. What user intent does this support?
   - Browser tasks that need reliable locator, DOM-id, coordinate, type,
     extract, wait, and screenshot action behavior through the CLI lane.
2. What autonomy level can it run at?
   - L1-L3 only. This phase proves fixtures; it does not grant broader
     autonomy or irreversible action permission.
3. What is the canonical truth source?
   - uClaw's task/run/event model remains canonical. These fixtures prove the
     provider result artifacts and outputs that later task routing can record.
4. What TaskEvent entries does it emit?
   - None. This PR adds fixture coverage only.
5. What context does it read, and how is it cited?
   - It reads local fake Playwright fixture state and worker output. Artifacts
     are asserted through returned artifact refs and pack-local files.
6. What capability cards does it add or consume?
   - It consumes the existing `browser.playwright_cli` declarative action and
     artifact capability contract; it adds no new provider card.
7. What policy hooks can block it?
   - Feature flag, runtime readiness, timeout, unsupported action, and future
     browser action policy hooks. This phase validates only worker fixture
     behavior under the existing gates.
8. What world projection does the UI render?
   - No UI change. The fixture outputs are shaped for later projection and
     harness scorecards.
9. What harness cases prove it works?
   - Focused Rust worker-script tests cover semantic locator, uClaw DOM id,
     coordinate fallback, non-screenshot actions not writing screenshot
     artifacts, type output, extract output, wait output, screenshot artifact
     refs, failure envelope, timeout kill, and no raw script.
10. What is the rollback or disable path?
   - Revert this PR or disable `playwright_cli`. No production behavior depends
     on these added tests.
11. What does it deliberately not own?
   - Provider promotion, BrowserProvider parity routing, agent-loop wiring,
     Settings/IPC, DB migrations, browser identity, Playwright MCP, raw script
     execution, and hosted providers.

## Allowed Files

- `src-tauri/src/browser/playwright_cli.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5e-cli-fixture-gates.md`

## Non-Goals

- No production worker behavior changes unless a fixture exposes an actual bug.
- No `agentic_loop.rs`, `tauri_commands.rs`, UI, IPC, DB migration, provider
  promotion, global npm path, or user data mutation.

## Impact Targets

- This slice adds tests around the existing Playwright CLI worker and provider
  adapter. If production symbols must change, run GitNexus impact first and
  stop on HIGH/CRITICAL.

## Implementation Plan

1. Add worker-script tests for semantic locator, uClaw DOM id, and coordinate
   fallback outputs.
2. Add a risk screenshot policy fixture proving a non-screenshot action does
   not emit screenshot artifact refs even when an artifact directory is present.
3. Add type/extract/wait output fixtures for the remaining declarative actions.
4. Update tracker, run focused/default verification, GitNexus staged detect,
   commit, push, and open PR.

## Rollback

Revert this PR. Phase 5A-5D implementation remains unchanged.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`
- `git diff --check -- src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5e-cli-fixture-gates.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5e-cli-fixture-gates`

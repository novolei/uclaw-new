# Phase 5F - Playwright CLI Action State Diff

## Goal

Close the remaining ADR Phase 5 gap for action result plus DOM/state diff on
stable locator clicks, type, and wait actions. This keeps the CLI lane behind
the feature flag and does not promote it into task routing.

## ADR Section 18 Questions

1. What user intent does this support?
   - Browser tasks that need auditable click, type, and wait outcomes without
     relying on screenshots after every action.
2. What autonomy level can it run at?
   - L1-L3 only. This phase adds evidence to bounded actions, not broader
     permission.
3. What is the canonical truth source?
   - uClaw task/run/event truth remains canonical. The worker returns compact
     state-diff evidence that later routing can record.
4. What TaskEvent entries does it emit?
   - None in this PR. It only enriches provider/worker results.
5. What context does it read, and how is it cited?
   - It reads page URL/title/body text summary and active element metadata from
     the app-managed Playwright page. It returns hashed/length summaries, not
     raw page text.
6. What capability cards does it add or consume?
   - It consumes the existing `browser.playwright_cli` declarative action
     contract and does not add a new provider card.
7. What policy hooks can block it?
   - Existing feature flag, runtime readiness, action timeout, unsupported
     action, and future browser action policy hooks still apply.
8. What world projection does the UI render?
   - No UI change. The compact diff is shaped for later TaskEvent/projection
     work.
9. What harness cases prove it works?
   - Focused worker-script fixtures assert click, type, and wait outputs include
     compact state diff evidence while existing screenshot/failure/timeout
     fixtures continue to pass.
10. What is the rollback or disable path?
   - Revert this PR or disable `playwright_cli`. The provider remains
     feature-flagged.
11. What does it deliberately not own?
   - Provider promotion, task routing, IPC, UI, DB migrations, browser identity,
     Playwright MCP, raw script execution, and hosted providers.

## Allowed Files

- `src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`
- `src-tauri/src/browser/playwright_cli.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5f-action-state-diff.md`

## Non-Goals

- No provider promotion, BrowserProvider parity routing, agent-loop wiring,
  Settings/IPC, DB migration, identity/profile UX, Playwright MCP, global npm,
  or user-installed Playwright production path.
- No raw page text in action diff output.

## Impact Targets

- `runAction` in the managed Playwright worker: GitNexus impact LOW, one direct
  caller (`main`), 0 affected processes.
- `write_fake_playwright_module` and focused worker fixture tests: GitNexus
  impact LOW, test-only blast radius.

## Implementation Plan

1. Add a compact before/after state snapshot helper in the worker.
2. Include `stateDiff` in click, type, and wait action outputs.
3. Extend fake Playwright fixtures and Rust tests to assert the diff shape.
4. Update tracker, run focused/default verification, GitNexus staged detect,
   commit, push, and open PR.

## Rollback

Revert this PR. Phase 5A-5E remain intact; the CLI provider still works without
compact diff evidence.

## Verification

- `node --check src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`
- `git diff --check -- src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5f-action-state-diff.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`

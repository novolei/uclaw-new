# Phase 5A - Playwright CLI Provider Contract

## Summary

Phase 5A starts ADR Phase 5 with a pure Rust contract and readiness shell for
the Playwright CLI thin lane. It defines the declarative action envelope,
addressing order, provider readiness from feature flags plus runtime-pack
status, and serialization tests. It does not spawn Node, launch Playwright,
run browser actions, promote the provider, add IPC, or execute runtime-pack
side effects.

## ADR Section 18 Questions

1. **User intent:** enable a future fast local Playwright CLI lane without
   asking users to install global npm packages or understand CLI tooling.
2. **Autonomy level:** L0/L1. This phase classifies readiness and builds typed
   request envelopes only; no autonomous browser action is executed.
3. **Canonical truth source:** Rust contracts in `browser::playwright_cli`
   plus existing `BrowserRuntimeFeatureFlags` and
   `BrowserRuntimePackStatusReport`.
4. **TaskEvent entries:** none emitted. Future Phase 5 execution slices may emit
   provider/action events after a runner exists.
5. **Context read/citation:** reads only in-memory feature flags and
   runtime-pack status supplied by callers; no external context is cited.
6. **Capability cards:** consumes the existing `browser.playwright_cli`
   capability card and exposes a matching readiness shell.
7. **Policy hooks:** feature flag defaults keep the provider disabled; raw
   scripts are unrepresentable in the v1 action enum.
8. **World projection:** no projection mutation; readiness output can later feed
   Browser Runtime Settings and provider routing.
9. **Harness cases:** unit tests cover feature-flag disabled, missing runtime
   pack, ready runtime pack, envelope serialization, addressing order, and no
   raw script action variant.
10. **Rollback/disable path:** revert this PR. The existing disabled
    Playwright CLI provider card remains metadata-only.
11. **Does not own:** child process runner, Node/Playwright startup, IPC,
    provider promotion/default selection, TaskEvents, DB migrations, Settings UI,
    runtime-pack download/extract/delete/promote, Browser Identity, MCP, or
    hosted providers.

## Allowed Files

- `src-tauri/src/browser/playwright_cli.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5a-cli-provider-contract.md`

## Non-Goals

- No Playwright or Node process spawn.
- No worker script file, stdout parser loop, timeout/kill implementation, or
  retry runner.
- No runtime-pack mutation, download, extraction, cleanup, rollback, or
  promotion.
- No provider promotion ahead of chromiumoxide.
- No IPC, Settings UI, TaskEvents, DB migrations, `tauri_commands.rs`,
  `agentic_loop.rs`, or task dispatch rewiring.
- No raw arbitrary Playwright script support.

## Impact Targets

- `src-tauri/src/browser/mod.rs`: additive module export only. GitNexus impact
  checked through `BrowserService` in the same file: LOW, 0 affected processes.
- `src-tauri/src/browser/playwright_cli.rs`: new pure module.
- Tracker and this plan are docs-only.

## Implementation Plan

1. Add `browser::playwright_cli` with declarative v1 actions:
   `navigate`, `click`, `type`, `screenshot`, `extract`, and `wait`.
2. Encode addressing order as first-class typed variants:
   semantic locator, uClaw DOM element id, then coordinates.
3. Add a request envelope containing schema version, request id, action,
   timeout, artifact policy, and app-managed runtime environment.
4. Add readiness builder that reports unavailable when `playwright_cli` flag is
   off, needs setup when runtime pack is not ready, and ready only when both the
   flag and runtime pack are ready.
5. Export the module from `browser::mod`.
6. Add focused unit tests.
7. Update the Browser Runtime tracker to close Phase 4, record PR #450, and mark
   Phase 5A in progress.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs src-tauri/src/browser/mod.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the Phase 5A commit. No runtime files, browser sessions, provider
selection, settings, task checkpoints, database rows, or user data are changed.

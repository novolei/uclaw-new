# Phase 5B - Playwright CLI Child Worker Boundary

## Summary

Start the real Playwright CLI thin lane by adding a supervised short-lived
child-worker runner for the existing v1 JSON envelope. This phase proves the
Rust-owned process boundary, app-managed runtime paths, stdin/stdout result
contract, timeout kill path, and fail-closed path validation without provider
promotion or task routing.

## ADR 18 Questions

1. **User intent:** run selected browser fixture actions through a faster local
   Playwright CLI lane once runtime setup is ready.
2. **Autonomy level:** L1-L3 only; this slice is an internal execution boundary
   and does not expose a new default provider.
3. **Canonical truth:** uClaw browser task/run/event/artifact model remains
   canonical. Worker stdout is an execution result, not product truth by itself.
4. **TaskEvents:** none emitted in this slice. Future wiring will emit Browser
   runtime/provider/action/artifact TaskEvents.
5. **Context read/cited:** consumes the existing request envelope and runtime
   pack paths/env from `BrowserRuntimePackStatusReport`.
6. **Capability cards:** consumes existing `browser.playwright_cli` capability
   card and feature flag; adds no new capability card.
7. **Policy hooks:** runtime must be ready, node/worker paths must stay inside
   the app-managed pack, action timeout must bound execution, and raw script
   remains unavailable.
8. **World projection:** no new UI projection. Future provider wiring will
   project child-worker success/failure through Browser runtime events.
9. **Harness cases:** focused Rust tests cover app-managed path enforcement,
   stdin/stdout result parsing, non-zero/invalid output classification, and
   timeout kill behavior.
10. **Rollback/disable:** revert this PR. The provider remains unpromoted and
    feature-flagged.
11. **Non-ownership:** no Tauri IPC command, no Settings UI, no task-loop
    routing, no provider promotion, no DB migration, no `agentic_loop.rs`, and
    no `tauri_commands.rs`.

## Allowed Files

- `src-tauri/src/browser/playwright_cli.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-cli-child-worker.md`

## Non-Goals

- Do not make Playwright CLI the default provider.
- Do not add a Tauri command or frontend invoke.
- Do not mutate runtime packs.
- Do not add raw Playwright script execution.
- Do not route Agent/browser tasks through this runner yet.

## Impact Targets

- GitNexus impact for `PlaywrightCliRequestEnvelope`: LOW, 1 direct caller, 0
  affected processes.
- GitNexus impact for `playwright_cli_provider_status`: LOW, 4 direct test
  callers, 0 affected processes.
- GitNexus impact for `build_playwright_cli_request_envelope`: LOW, 2 direct
  test callers, 0 affected processes.

## Implementation Steps

1. Define a child-worker config derived from app-managed runtime paths.
2. Define a result envelope and structured worker error classifications.
3. Add an async runner that spawns the runtime-pack node binary with the worker
   script, writes one JSON request to stdin, reads one JSON result from stdout,
   captures stderr, and kills on timeout.
4. Add tests using temp app-managed fixture scripts, including path escape and
   timeout coverage.
5. Update tracker with Phase 5B status, verification, and next action.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs src-tauri/src/browser/mod.rs`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect-changes`

## Rollback

Revert this PR. The contract-only provider readiness and request envelope from
Phase 5A remain intact; no production task routing depends on this runner yet.

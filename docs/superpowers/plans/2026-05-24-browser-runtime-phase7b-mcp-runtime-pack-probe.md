# Phase 7B - MCP Runtime-Pack Probe

## Scope

Phase 7B extends the app-managed Browser runtime-pack probe with explicit
`@playwright/mcp` package evidence. This prevents future MCP sidecar work from
falling back to global npm or user-installed Playwright paths.

Allowed files:

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7b-mcp-runtime-pack-probe.md`
- `src-tauri/src/browser/playwright_cli.rs` (test-helper shape only)
- `src-tauri/src/browser/runtime_pack.rs`
- `src-tauri/src/browser/runtime_pack_runner.rs` (test fixture only)
- `src-tauri/src/browser/runtime_pack_tests.rs`

## ADR Section 18 Questions

1. User-visible promise: Playwright MCP remains disabled until the app-managed
   runtime pack can prove the pinned MCP package is present.
2. Autonomy tier: L1 local readiness metadata only; no sidecar spawn or browser
   action execution.
3. Canonical truth source: runtime-pack status reports remain the canonical
   readiness evidence for local Playwright provider lanes.
4. TaskEvents: none emitted in this slice.
5. Context and citations: only safe filesystem presence booleans/path metadata;
   no MCP output or page content enters context.
6. Capability cards: supports the existing `browser.playwright_mcp` card by
   making runtime-pack MCP package presence observable.
7. Policy hooks: later MCP provider selection can block when
   `playwright_mcp_package_present` is false.
8. World projection: no UI changes; later Settings/Doctor views can display MCP
   package readiness from the status report.
9. Harness cases: focused runtime-pack tests prove path derivation, probe
   serialization, ready-pack detection, and that missing MCP does not break CLI
   readiness.
10. Rollback/disable path: revert this PR; MCP package presence is no longer
    tracked, while existing CLI runtime readiness remains unchanged.
11. Deliberately not owned: no MCP spawn, no package download/install, no IPC,
    no Settings UI, no TaskEvents, no provider routing/promotion, no DB
    migration, and no global npm fallback.

## Impact Targets

- `BrowserRuntimePackManifest`: LOW GitNexus impact before edits.
- `BrowserRuntimePackPaths`: LOW GitNexus impact before edits.
- `BrowserRuntimePackProbe`: MEDIUM GitNexus impact before edits, limited to
  browser runtime-pack tests/status code; no affected execution processes.
- `probe_runtime_pack_filesystem`: LOW GitNexus impact before edits.
- `diagnose_runtime_pack`: MEDIUM GitNexus impact before edits; this phase
  deliberately does not make missing MCP package a general runtime-pack doctor
  failure so existing CLI readiness stays intact.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/playwright_cli.rs`
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7b-mcp-runtime-pack-probe.md src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/playwright_cli.rs`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the PR. No runtime files are created, deleted, downloaded, or launched.

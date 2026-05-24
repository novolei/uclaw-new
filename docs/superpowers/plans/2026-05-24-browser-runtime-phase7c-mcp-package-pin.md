# Phase 7C - MCP Package Pin Correction

## Scope

Phase 7C corrects the pinned app-managed `@playwright/mcp` package version
before the supervised sidecar runner is added. Phase 7B introduced MCP package
evidence but reused the Playwright core version (`1.53.0`), while npm metadata
shows the current stable `@playwright/mcp` package is `0.0.75` and exposes the
`playwright-mcp` bin at `cli.js`.

Allowed files:

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7c-mcp-package-pin.md`
- `src-tauri/src/browser/playwright_mcp.rs` (tests and package-spec
  expectation only)
- `src-tauri/src/browser/runtime_pack.rs`
- `src-tauri/src/browser/runtime_pack_tests.rs`

## ADR Section 18 Questions

1. User-visible promise: MCP setup remains app-managed and pinned to a real
   package version before sidecar launch work begins.
2. Autonomy tier: L1 metadata correction only; no sidecar spawn or browser
   action execution.
3. Canonical truth source: runtime-pack manifest evidence remains canonical for
   local Playwright provider package versions.
4. TaskEvents: none emitted in this slice.
5. Context and citations: no MCP output, page content, or raw tool data enters
   context.
6. Capability cards: supports the existing `browser.playwright_mcp` card by
   making the pinned package spec resolvable.
7. Policy hooks: later runner/promotion gates can require this real package pin
   before enabling MCP.
8. World projection: no UI changes.
9. Harness cases: focused tests prove manifest defaulting and sidecar package
   spec use the corrected MCP package version.
10. Rollback/disable path: revert this PR; MCP package evidence returns to the
    prior pin while the provider stays feature-flagged and disabled by default.
11. Deliberately not owned: no MCP spawn, package install/download, IPC,
    Settings UI, TaskEvents, artifact writes, provider routing/promotion, DB
    migration, hosted provider, or global npm fallback.

## Impact Targets

- `default_playwright_mcp_version`: LOW GitNexus impact before edits, 0 direct
  dependants and 0 affected processes.
- `build_playwright_mcp_sidecar_spec`: LOW GitNexus impact before edits, 4
  direct test callers and 0 affected processes; this phase updates tests only
  around its package-spec output.
- `runtime_pack_manifest_versions_match`: HIGH GitNexus impact when considered
  directly, so this phase does not edit that function. The pin correction is
  limited to the manifest default and focused tests.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/playwright_mcp.rs`
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7c-mcp-package-pin.md src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/playwright_mcp.rs`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the PR. No runtime files are created, deleted, downloaded, launched, or
promoted.

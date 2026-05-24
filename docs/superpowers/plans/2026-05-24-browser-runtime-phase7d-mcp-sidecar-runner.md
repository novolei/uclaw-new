# Phase 7D - MCP Sidecar Runner

## Scope

Phase 7D adds the supervised Playwright MCP process boundary behind the
existing `browser.playwright_mcp` contract. It starts MCP only from the
app-managed runtime pack's Node binary and pack-local
`node_modules/@playwright/mcp/cli.js`; `npx`, global npm, and user-installed
Playwright remain excluded from the production path.

Allowed files:

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7d-mcp-sidecar-runner.md`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/playwright_mcp.rs`
- `src-tauri/src/browser/playwright_mcp_sidecar.rs`

## ADR Section 18 Questions

1. User-visible promise: MCP can become a richer local provider without a hidden
   second browser truth source or global npm dependency.
2. Autonomy tier: L2 supervised local process boundary only; no task routing or
   provider promotion.
3. Canonical truth source: the Browser runtime pack remains canonical for Node,
   MCP package location, package pin, profile dir, and artifact dir.
4. TaskEvents: none emitted in this slice; later routing slices must map MCP
   execution into TaskEvent/artifact surfaces.
5. Context and citations: no raw MCP tool output enters model context here.
6. Capability cards: supports the existing `browser.playwright_mcp` capability
   card with a real sidecar start boundary.
7. Policy hooks: feature flag and runtime readiness stay enforced by the
   envelope builder; runner validates paths remain inside the app-managed pack.
8. World projection: no UI changes.
9. Harness cases: focused fixture tests cover app-managed Node/CLI paths, no
   `npx` package arg, global-node rejection, and startup-exit handling.
10. Rollback/disable path: revert this PR; MCP returns to contract/probe-only
    state and remains feature-flagged.
11. Deliberately not owned: no raw MCP tool exposure, MCP protocol client,
    provider selection, Settings IPC/UI, TaskEvents, DB migration, hosted
    provider, package installation/download, or task dispatch.

## Impact Targets

- `build_playwright_mcp_sidecar_spec`: LOW GitNexus impact before edits, 4
  direct test callers and 0 affected processes.
- `PlaywrightMcpSidecarSpec.args`: LOW GitNexus impact before edits, 1 direct
  test caller and 0 affected processes. This phase changes it from npx-style
  package args to pack-local MCP CLI flags.
- `playwright_mcp_provider_status`: LOW GitNexus impact before edits, 3 direct
  test callers and 0 affected processes.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs src-tauri/src/browser/playwright_mcp_sidecar.rs`
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7d-mcp-sidecar-runner.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp.rs src-tauri/src/browser/playwright_mcp_sidecar.rs`
- GitNexus `detect_changes` before commit.

## Rollback

Revert this PR. No real runtime pack files are created, no packages are
downloaded, no provider is promoted, and no task dispatch path changes.

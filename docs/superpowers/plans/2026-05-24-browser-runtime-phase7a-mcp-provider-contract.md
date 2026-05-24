# Phase 7A - Playwright MCP Provider Contract

## Scope

Phase 7A starts ADR Phase 7 with a reversible contract slice for
`browser.playwright_mcp`. It adds typed provider readiness metadata, a pinned
sidecar specification shape, and a uClaw-level request envelope that blocks raw
MCP tool exposure by default.

Allowed files:

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7a-mcp-provider-contract.md`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/playwright_mcp.rs`

## ADR Section 18 Questions

1. User-visible promise: Playwright MCP becomes a disabled-by-default provider
   lane for snapshots, locator discovery, traces, and exploratory automation,
   without becoming the main browser truth source.
2. Autonomy tier: this slice stays L1 metadata/contracts only; later sidecar
   execution remains policy-gated and local-first.
3. Canonical truth source: uClaw browser task runs, steps, artifacts, and
   provider status remain canonical. MCP internal tool state is execution detail.
4. TaskEvents: none emitted in Phase 7A. Later Phase 7 slices must map MCP
   artifacts/errors through browser provider/runtime events.
5. Context and citations: the contract names uClaw-level actions and artifact
   refs only; no raw MCP snapshot/tool output enters model context here.
6. Capability cards: consumes existing `browser.playwright_mcp` card and adds a
   focused provider readiness/envelope contract for snapshot, locator discovery,
   trace, navigate, click, and type actions.
7. Policy hooks: feature flag, runtime readiness, pinned package version,
   controlled profile/output directories, no raw MCP exposure, and profile mode
   checks can block execution.
8. World projection: no UI changes; later projection work should show MCP
   readiness, sidecar health, artifact pack refs, and degraded/disabled state.
9. Harness cases: focused unit tests prove disabled fallback, runtime gating,
   controlled sidecar args, raw-tool blocking, storage-state validation, and
   uClaw-level envelope serialization.
10. Rollback/disable path: revert this PR or keep `playwright_mcp` feature flag
    off. No runtime process, filesystem mutation, or provider promotion is added.
11. Deliberately not owned: no MCP spawn, no MCP manager registration, no raw
    MCP tools, no Settings UI, no TaskEvents, no provider routing/promotion, no
    DB migration, no hosted provider, and no global npm/user-installed
    Playwright path.

## Impact Targets

- `BrowserRuntimeFeatureFlags`: LOW GitNexus impact before edits.
- `browser_provider_capability_card`: LOW GitNexus impact before edits.
- `BROWSER_PROVIDER_CAPABILITY_CARDS`: LOW GitNexus impact before edits.
- `local_chromium_capabilities`: LOW GitNexus impact before edits; used only as
  a provider-contract comparison point.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs`
- `src-tauri/src/browser/mod.rs` is intentionally kept to the two module/export
  lines because the pre-existing legacy file is not full-file rustfmt-clean;
  full formatting would touch unrelated `BrowserService` code.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7a-mcp-provider-contract.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp.rs`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the PR. Because Phase 7A adds no process spawn, IPC, migrations, or
runtime-pack mutation, rollback only removes the MCP contract module and tracker
entries.

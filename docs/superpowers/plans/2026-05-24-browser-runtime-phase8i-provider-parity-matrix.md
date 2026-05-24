# Phase 8I - Provider Parity Matrix Harness

## Goal

Close the next ADR Phase 8 gate by adding a model-free provider parity matrix
that proves the same browser harness case can route across local Chromium,
Playwright CLI, Playwright MCP where appropriate, and a mock hosted provider,
with fallback preserving artifact visibility.

## ADR Section 18 Questions

1. What user intent does this support?
   - Evidence-backed browser provider choice and reversible fallback for browser
     automation tasks.
2. What autonomy level can it run at?
   - Harness-only, model-free verification. It does not raise live browser task
     autonomy or promote providers.
3. What is the canonical truth source?
   - Provider capability cards, provider route decisions, and harness artifacts.
4. What TaskEvent entries does it emit?
   - No new runtime TaskEvent names. The matrix can be attached as a harness
     JSON artifact.
5. What context does it read, and how is it cited?
   - Static provider capability cards and synthesized provider status probes.
     The report cites provider ids, route decisions, artifact policies, and
     fallback providers.
6. What capability cards does it add or consume?
   - It consumes local Chromium, Playwright CLI, Playwright MCP, and hosted
     provider cards; it adds no new provider card.
7. What policy hooks can block it?
   - Provider disabled IDs, provider readiness, card-supported actions, and
     route ranking/fallback rules.
8. What world projection does the UI render?
   - No UI change. This phase creates harness artifact data that future
     scorecard/projection surfaces can consume.
9. What harness cases prove it works?
   - Default model-free shared navigate/click cases select each expected
     provider when explicitly isolated, include mock hosted, and prove disabling
     a previous provider falls back with artifact policy preserved.
10. What is the rollback or disable path?
   - Revert this PR. Live routing and provider execution from Phase 8H remain
     unchanged.
11. What does it deliberately not own?
   - Provider promotion, live default selection changes, real hosted execution,
     MCP live execution, UI/IPC/DB, Settings toggles, raw tools/scripts, or
     runtime pack mutation.

## Allowed Files

- `src-tauri/src/harness/adapters/browser_provider.rs`
- `src-tauri/src/harness/adapters/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8i-provider-parity-matrix.md`

## Non-Goals

- Do not change provider route ranking.
- Do not promote Playwright CLI, MCP, or hosted providers.
- Do not execute real hosted providers or MCP sidecars.
- Do not add UI, IPC, Settings, DB, or runtime-pack mutation.
- Do not touch `agent_loop.rs` or `tauri_commands.rs` in this slice.

## Impact Targets

- New module: `src-tauri/src/harness/adapters/browser_provider.rs`
- Module export: `src-tauri/src/harness/adapters/mod.rs`
- Read-only dependencies:
  `BrowserProviderRouteRequest`, `decide_browser_provider_route`,
  `browser_provider_capability_cards`

Pre-edit GitNexus impact reported MEDIUM for the existing route/card helpers
as read-only dependencies and no HIGH/CRITICAL risk. The implementation does
not modify those helpers.

## Rollback

Revert this PR. The provider execution boundary and route decisions from Phase
8H stay intact, and no runtime state or user data is changed.

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser_provider
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/harness/adapters/browser_provider.rs src-tauri/src/harness/adapters/mod.rs
git diff --check -- src-tauri/src/harness/adapters/browser_provider.rs src-tauri/src/harness/adapters/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8i-provider-parity-matrix.md
GitNexus detect_changes scope=staged
```

# Phase 8J - Provider Default Policy Gate

## Goal

Close the remaining ADR Phase 8 default-selection requirement by adding a pure,
data-driven, reversible provider default policy gate. The gate can retain the
current local Chromium default, identify a strictly better promotion candidate,
or select an artifact-visible fallback when the current default is disabled,
without mutating live settings or promoting any provider by default.

## ADR Section 18 Questions

1. What user intent does this support?
   - Safer browser automation provider evolution: users get reversible,
     evidence-backed provider defaults instead of hidden code forks.
2. What autonomy level can it run at?
   - Policy computation only. It does not raise task autonomy or execute
     provider actions.
3. What is the canonical truth source?
   - Provider capability cards, harness score metadata, explicit default
     evidence, disabled-provider policy, and rollback provider id.
4. What TaskEvent entries does it emit?
   - None in this slice. Existing provider route events remain unchanged.
5. What context does it read, and how is it cited?
   - Static provider cards and supplied default evidence. The decision cites
     provider ids, reliability basis points, artifact visibility, policy
     boundary evidence, local-first evidence, and blocked reasons.
6. What capability cards does it add or consume?
   - It consumes existing provider cards and adds no new card.
7. What policy hooks can block it?
   - Disabled providers, missing/default evidence, insufficient reliability,
     missing artifact visibility, missing policy-boundary metric, missing
     local-first metric, and hosted-default disallow.
8. What world projection does the UI render?
   - No UI change. Future UI/IPC may render this policy decision, but this PR
     only defines the backend contract.
9. What harness cases prove it works?
   - Focused Rust tests prove current local retention, stricter promotion with
     rollback, hosted default blocking, and disabled-current fallback with
     artifact visibility.
10. What is the rollback or disable path?
   - Revert this PR. Existing route decisions, live provider execution, and
     Phase 8I parity artifacts remain intact.
11. What does it deliberately not own?
   - Live settings mutation, actual default promotion, route ranking changes,
     provider execution, MCP execution, hosted execution, UI, IPC, DB
     migration, runtime-pack mutation, or task-loop behavior.

## Allowed Files

- `src-tauri/src/browser/provider_defaults.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8j-provider-default-policy.md`

## Non-Goals

- Do not change `rank_browser_provider_candidates` or live route ranking.
- Do not set Playwright CLI, Playwright MCP, hosted, or raw CDP as a new live
  default.
- Do not add UI, IPC, Settings controls, DB migration, runtime-pack mutation,
  real provider execution, or TaskEvent emission.
- Do not touch `agent_loop.rs` or `tauri_commands.rs`; this is not a live task
  routing slice.

## Impact Targets

- New module: `src-tauri/src/browser/provider_defaults.rs`
- Module export: `src-tauri/src/browser/mod.rs`
- Read-only dependencies:
  `BrowserProviderCapabilityCard`, `browser_provider_capability_cards`,
  `BrowserProviderLane`, and local provider ids.

Pre-edit GitNexus impact reported LOW for `BrowserProviderCapabilityCard` and
MEDIUM for read-only helper dependencies `browser_provider_capability_cards`
and `rank_browser_provider_candidates`; no HIGH/CRITICAL risk was observed.
`browser/mod.rs` is not indexed as a symbol target and returned UNKNOWN, so
the edit is kept to one additive module export.

## Rollback

Revert this PR. No runtime state, settings, provider defaults, artifacts, or
user data are mutated.

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check src-tauri/src/browser/provider_defaults.rs
rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/browser/mod.rs
git diff --check -- src-tauri/src/browser/provider_defaults.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8j-provider-default-policy.md
GitNexus detect_changes scope=staged
```

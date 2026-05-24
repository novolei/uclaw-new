# Browser Runtime Phase 8A - Provider Route Decision

## Context

PR #474 merged Phase 7G and encoded the MCP-vs-CLI ranking rule. ADR Phase 8
now needs provider choice to become a runtime policy decision backed by status,
capability cards, and reversible events. This slice adds the pure route decision
contract before live task routing.

## ADR Section 18 Questions

1. **Intent:** Add a provider route decision contract that combines provider
   status/readiness with Phase 7 selection metadata.
2. **Autonomy:** No autonomous browser behavior changes. The decision is not
   wired into agent-loop execution yet.
3. **Truth source:** Existing `BrowserProviderStatus` plus provider capability
   cards remain the source of provider facts.
4. **TaskEvent:** No events are emitted, but the decision records
   `browser.provider.selected`, `browser.provider.degraded`, and
   `browser.provider.rolled_back` event intentions for the future emitter.
5. **Context:** Consumes only a selection request, disabled provider ids,
   optional previous provider id, and provider status snapshots.
6. **Capability:** Uses action/observation eligibility from capability cards
   and readiness from provider statuses.
7. **Hooks:** No live policy hook is changed; disabled providers are modeled as
   input data.
8. **Projection:** No projection write in this PR.
9. **Harness:** Unit tests cover ready selection, degraded fallback, disabled
   provider fallback, and no-provider blocking.
10. **Rollback:** Revert this PR. Phase 7 ranking and provider status surfaces
    remain available.
11. **Does not own:** Live provider routing, agent-loop integration,
    `tauri_commands.rs`, UI, IPC, DB, provider promotion, scorecard storage,
    hosted provider implementation, or artifact persistence.

## Allowed Files

- `src-tauri/src/browser/provider.rs`
- `src-tauri/src/browser/provider_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8a-provider-route-decision.md`

## Non-Goals

- Do not call provider workers or mutate browser sessions.
- Do not wire `agentic_loop.rs` or `tauri_commands.rs`.
- Do not emit TaskEvents or write projections.
- Do not promote Playwright CLI/MCP as default.
- Do not add UI, IPC, DB migration, hosted provider behavior, or network work.

## Impact Targets

- GitNexus impact for `BrowserProviderStatus`: LOW, 0 affected processes.
- GitNexus impact for `BrowserProviderCapabilities`: LOW, 3 direct callers, 0
  affected processes.
- GitNexus impact for `BrowserProviderReadiness`: LOW, 0 affected processes.
- GitNexus impact for `BrowserProviderSelectionRequest`: LOW, 4 direct test
  callers, 0 affected processes.

## Implementation

- Add `BrowserProviderRouteRequest`.
- Add route candidate/event/decision DTOs.
- Add `decide_browser_provider_route` over existing status snapshots.
- Preserve Phase 7 ranking: MCP does not outrank CLI unless the request says
  MCP-specific capability is required.
- Record provider selected/degraded/rollback event intentions without emitting
  them.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `rustfmt --edition 2021 --check src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is low because this is additive contract/test/doc work only. Rollback is a
single PR revert; no runtime state, provider process, user data, or routing
behavior changes.

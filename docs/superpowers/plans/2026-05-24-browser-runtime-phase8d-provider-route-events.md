# Browser Runtime Phase 8D - Provider Route Events

## Context

PR #477 merged Phase 8C and added explicit provider scorecard evidence to the
provider capability cards. ADR Phase 8 still requires provider selection,
degradation, and rollback events. Phase 8D materializes Phase 8A/8B route
event intents into canonical Browser-source `TaskEvent`s without changing live
provider defaults, running providers, or promoting any lane.

## ADR Section 18 Questions

1. **Intent:** Convert provider route decision intents into rollout-visible
   task events.
2. **Autonomy:** No autonomous browser behavior changes.
3. **Truth source:** Provider router decisions remain the route truth; rollout
   `TaskEvent`s are an observable projection of those decisions.
4. **TaskEvent:** Add a generic `TaskEvent::Signal` record and map provider
   selected, degraded, and rolled-back intents into Browser-source signals with
   the browser event name as `code`.
5. **Context:** No runtime context is read or written.
6. **Capability:** Scorecard fields remain evidence metadata; fixture counts
   are not treated as runtime quality scores.
7. **Hooks:** No policy hook changes.
8. **Projection:** No world projection mutation.
9. **Harness:** Focused tests assert selected, degraded, and rollback route
   intents produce stable task events and preserve provider/reason metadata.
10. **Rollback:** Revert this PR; Phase 8A route decisions, Phase 8B router
    state, and Phase 8C scorecards remain intact.
11. **Does not own:** Agent-loop provider execution, Tauri IPC, UI, DB,
    provider default promotion, provider disable persistence, hosted providers,
    or score computation from fixture counts.

## Allowed Files

- `src-tauri/src/browser/rollout_bridge.rs`
- `src-tauri/src/browser/rollout_bridge_tests.rs`
- `crates/uclaw-runtime-contracts/src/lib.rs`
- `crates/uclaw-runtime-contracts/src/contracts_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8d-provider-route-events.md`

## Non-Goals

- Do not switch live browser actions to the provider router.
- Do not promote CLI, MCP, raw CDP, or hosted providers.
- Do not change `agentic_loop.rs` or `tauri_commands.rs`.
- Do not write DB migrations, settings, or persistent provider state.
- Do not treat Phase 8C fixture counts as live quality scores.

## Impact Targets

- GitNexus impact for `BrowserProviderRouteEventIntent`: LOW, 1 direct caller,
  0 affected processes.
- GitNexus impact for `browser_run_to_events`: MEDIUM, 9 direct callers, 1
  affected process, used as nearby bridge context only. This phase does not
  change `browser_run_to_events`.
- GitNexus impact for `TaskEvent` enum and impl: LOW, 0 affected processes.

## Implementation

- Add a pure conversion helper in the browser rollout bridge from
  `BrowserProviderRouteDecision` event intents to canonical `TaskEvent`s.
- Add `TaskEvent::Signal` so normal provider selection events do not pollute
  warning counts.
- Encode provider id, route status, selected provider, and reason in a compact
  JSON message so artifacts and rollout logs can explain selection decisions.
- Add focused tests for selected, degraded, and rollback route event mapping.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml -p uclaw-runtime-contracts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
- `rustfmt --edition 2021 --check src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is low to medium by symbol proximity but low behaviorally: the helper is
pure, additive, and not called by live provider execution yet. Rollback is a
single PR revert.

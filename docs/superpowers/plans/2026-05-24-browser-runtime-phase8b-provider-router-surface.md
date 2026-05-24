# Browser Runtime Phase 8B - Provider Router Surface

## Context

PR #475 merged Phase 8A and added a pure provider route decision contract.
Phase 8B adds a small in-memory router surface that owns provider status
snapshots, disabled provider ids, last selected provider id, and explicit
recovery provider id before live task routing touches agent-loop or IPC.

## ADR Section 18 Questions

1. **Intent:** Add a thin provider router surface that turns provider status
   snapshots into route decisions.
2. **Autonomy:** No live autonomous browser behavior changes.
3. **Truth source:** Existing provider status snapshots and Phase 8A route
   decision remain the source of truth.
4. **TaskEvent:** No events are emitted. The router only returns event
   intentions from the Phase 8A decision.
5. **Context:** Holds status snapshots, disabled provider ids, last selected
   provider id, and one-shot recovery provider id in memory.
6. **Capability:** Delegates capability/ranking to Phase 7G and readiness
   selection to Phase 8A.
7. **Hooks:** Disabled provider ids are modeled as policy input; no policy
   service is wired yet.
8. **Projection:** No projection write in this PR.
9. **Harness:** Unit tests cover status upsert, disable/enable fallback,
   explicit recovery-provider rollback, ordinary provider switching, and
   missing-status blocking.
10. **Rollback:** Revert this PR. Phase 8A pure decisions remain available.
11. **Does not own:** Agent-loop routing, Tauri IPC, TaskEvent emission,
    scorecard storage, UI, DB, hosted providers, or provider process execution.

## Allowed Files

- `src-tauri/src/browser/provider.rs`
- `src-tauri/src/browser/provider_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8b-provider-router-surface.md`

## Non-Goals

- Do not edit `agentic_loop.rs` or `tauri_commands.rs`.
- Do not execute provider actions.
- Do not emit TaskEvents or mutate projections.
- Do not promote any provider as default.

## Impact Targets

- GitNexus impact for `BrowserProviderRouteRequest`: MEDIUM, 5 direct test
  callers, 0 affected processes.
- GitNexus impact for `decide_browser_provider_route`: MEDIUM, 5 direct test
  callers, 0 affected processes.
- GitNexus impact for `BrowserProviderStatus`: LOW, 0 affected processes.

## Implementation

- Add `BrowserProviderRouter`.
- Add status upsert/list helpers.
- Add disable/enable helpers.
- Add explicit recovery-provider setter and route method.
- Keep all behavior in memory and deterministic.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is medium by symbol impact but low behaviorally: no live routing, no
processes, no filesystem changes, no IPC. Rollback is a single PR revert.

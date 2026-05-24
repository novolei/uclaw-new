# Browser Runtime Phase 8C - Provider Scorecard Contract

## Context

PR #476 merged Phase 8B and added the in-memory provider router. ADR Phase 8
still requires provider choice to be backed by parity scorecards, not code forks
or preferences. Phase 8C adds the scorecard contract to provider capability
cards without running providers, changing defaults, emitting events, or wiring
agent-loop/IPC paths.

## ADR Section 18 Questions

1. **Intent:** Add a typed provider harness score baseline to capability cards.
2. **Autonomy:** No live autonomous browser behavior changes.
3. **Truth source:** Capability cards stay the source of provider capability and
   selection metadata; scorecard fields are evidence metadata only.
4. **TaskEvent:** No events are emitted.
5. **Context:** No runtime context is read or written.
6. **Capability:** Scorecard metadata sits beside permissions, actions,
   observation modes, artifacts, and disable path.
7. **Hooks:** No policy hook changes.
8. **Projection:** No projection write in this PR.
9. **Harness:** Unit tests assert every provider card has harness subjects and
   explicit scorecard evidence, including the hosted provider disabled baseline.
10. **Rollback:** Revert this PR; Phase 8A/8B routing contracts remain intact.
11. **Does not own:** Agent-loop routing, Tauri IPC, TaskEvent emission, DB,
    UI, hosted provider implementation, provider execution, or default
    promotion.

## Allowed Files

- `src-tauri/src/browser/runtime_contracts.rs`
- `src-tauri/src/browser/runtime_contracts_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8c-provider-scorecard.md`

## Non-Goals

- Do not route live browser tasks.
- Do not promote any provider as default.
- Do not run provider actions or emit TaskEvents.
- Do not edit `agentic_loop.rs` or `tauri_commands.rs`.

## Impact Targets

- GitNexus impact for `BrowserProviderCapabilityCard`: LOW, 1 direct file
  caller, 0 affected processes.
- GitNexus impact for `browser_provider_capability_cards`: MEDIUM, 6 direct
  callers, 0 affected processes.

## Implementation

- Add `BrowserProviderHarnessScore`.
- Add `harness_score` to each `BrowserProviderCapabilityCard`.
- Keep current defaults: local Chromium is established; CLI/MCP are
  feature-lane baselines; hosted remains disabled/no fixture evidence.
- Extend tests to require explicit scorecard evidence and no raw/default hosted
  promotion.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is low to medium by symbol reach but low behaviorally: scorecards are
static metadata only. Rollback is a single PR revert.

# Browser Runtime Phase 7G - MCP Selection Policy

## Context

PR #473 merged Phase 7F and added provider-level MCP artifact/error routing.
ADR Phase 7 still says MCP must not outrank the Playwright CLI thin lane unless
the task requires MCP-specific capability. This phase adds that rule as a pure
contract and test surface only.

## ADR Section 18 Questions

1. **Intent:** Define a provider selection/ranking contract proving Playwright
   MCP remains behind the CLI thin lane unless MCP-specific observation or
   exploration is required.
2. **Autonomy:** No live autonomy change. The function is not wired into task
   routing.
3. **Truth source:** uClaw provider capability cards remain the source of
   provider facts.
4. **TaskEvent:** No TaskEvents are emitted. Future routing can emit
   `browser.provider.selected` from this decision metadata.
5. **Context:** Consumes only provider cards and an explicit selection request.
6. **Capability:** Uses action and observation-mode eligibility from provider
   capability cards. MCP-specific capability must be explicit.
7. **Hooks:** No policy hooks are changed; future live routing still needs flag,
   runtime, identity/profile, and permission gates.
8. **Projection:** No projection write in this PR.
9. **Harness:** Unit tests cover CLI outranking MCP by default, MCP outranking
   CLI only for MCP-specific needs, and ineligible provider exclusion.
10. **Rollback:** Revert this PR. Phase 7F provider result routing remains
    available, but the selection-rank contract disappears.
11. **Does not own:** Phase 8 provider router, default promotion, task routing,
    TaskEvent emission, UI/IPC, DB migration, runtime side effects, hosted
    providers, or raw MCP exposure.

## Allowed Files

- `src-tauri/src/browser/runtime_contracts.rs`
- `src-tauri/src/browser/runtime_contracts_tests.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7g-mcp-selection-policy.md`

## Non-Goals

- Do not select a provider for real Browser tasks.
- Do not promote MCP, CLI, or hosted providers.
- Do not edit `agentic_loop.rs`, `tauri_commands.rs`, Settings UI, IPC, DB
  migrations, or provider execution modules.
- Do not add network, filesystem, or process side effects.

## Impact Targets

- GitNexus impact for `BrowserProviderCapabilityCard`: LOW, 1 direct file
  touch, 0 affected processes.
- GitNexus impact for `browser_provider_capability_cards`: LOW, 1 direct test
  caller, 0 affected processes.
- GitNexus impact for `browser_provider_capability_card`: LOW, 1 direct test
  caller, 0 affected processes.
- GitNexus impact for `BrowserProviderLane`: LOW, 0 direct callers, 0 affected
  processes.

## Implementation

- Add a `BrowserProviderSelectionRequest` contract.
- Add `BrowserProviderSelectionCandidate` output metadata.
- Add `rank_browser_provider_candidates` over capability cards.
- Encode lane order so MCP stays behind CLI unless
  `requires_mcp_specific_capability` is true.
- Keep raw CDP and hosted providers lower than local/CLI/MCP in this pure
  ranking surface.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is low because this is additive contract logic and tests only. Rollback is
a single PR revert; no runtime state, provider process, user data, or task
routing behavior changes.

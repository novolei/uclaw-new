# Browser Runtime Phase 10A - Hosted Provider Capability Contract

## Phase Goal

Add the first ADR Phase 10 hosted-provider contract as pure Rust metadata and
policy evaluation. Hosted providers must remain opt-in escape hatches behind
`BrowserProvider`, explicit data-boundary policy, artifact visibility, cost
visibility, disable/fallback controls, and local-first defaults.

This phase does not add Browserbase, Browser Use Cloud, Steel, Hyperbrowser, or
any other real hosted SDK. It does not perform network calls, store credentials,
execute hosted browser actions, add UI/IPC/DB surfaces, or change live provider
defaults.

## ADR 11 Questions

1. User intent:
   Support hostile-site, proxy/isolation, scaling, CAPTCHA/manual takeover, and
   deployment-constraint browser tasks that local providers cannot satisfy.

2. Autonomy level:
   Hosted routes are L1-L3 only by default and require explicit policy gates for
   data egress, sensitive actions, manual takeover, file transfer, posting,
   account changes, purchases, and any L4+ autonomy.

3. Canonical truth source:
   uClaw task/run/event state, provider route decisions, artifact refs, and
   future TaskEvents stay canonical. Hosted provider state is implementation
   detail and cannot become product truth.

4. TaskEvent entries:
   This phase emits none. It defines event-intent/policy reasons for future
   provider selection, degradation, rollback, artifact-pack, data-boundary, and
   manual-takeover events.

5. Context read and citation:
   The contract reads capability cards, feature flags, explicit hosted-provider
   policy, request intent, artifact refs, cost estimate metadata, and local
   fallback availability. Any future model-visible hosted observation must cite
   artifact refs produced at the provider boundary.

6. Capability cards:
   Consume `browser.hosted` and add a dedicated hosted-provider capability
   contract that declares provider kind, data-boundary policy, profile/storage
   policy, artifact policy, cost policy, disable path, and fallback provider.

7. Policy hooks:
   Block when hosted providers are disabled, credentials are absent, data
   boundary prompt is missing or unaccepted, profile/storage policy is unsafe,
   artifact capture is absent, cost visibility is absent, local fallback is
   unavailable, request reason does not justify hosted routing, or sensitive
   side effects exceed approved autonomy.

8. World projection:
   No UI is added in this phase. Future projection must show hosted/local
   provider, data-boundary state, cost estimate, artifact refs, local fallback,
   and whether manual takeover is required.

9. Harness cases:
   Focused tests must cover hosted-provider disabled fallback,
   data-boundary prompt required, artifact capture required, cost visibility
   required, local fallback required, and an opt-in mock-hosted ready path.

10. Rollback or disable path:
   Revert this PR to remove the contract. The contract itself records
   `hosted_providers=false`, per-provider disabled ids, and local fallback
   provider id as the disable/rollback path.

11. Deliberately not owned:
   No vendor choice, credential storage, hosted SDK, live execution, UI/IPC/DB,
   route promotion, default-provider mutation, raw browser-side scripts, or
   agent-loop rewiring.

## Allowed Files

- `src-tauri/src/browser/hosted_provider.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/runtime_contracts.rs`
- `src-tauri/src/browser/runtime_contracts_tests.rs`
- `src-tauri/src/browser/provider_defaults.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase10a-hosted-provider-contract.md`

## Non-Goals

- No real hosted provider process, network call, SDK, API key, credential, or
  storage.
- No production provider execution path.
- No provider default or route ranking changes beyond preserving hosted as
  opt-in and last-ranked.
- No UI, Tauri IPC, Settings, DB migration, TaskEvent emission, or
  `agentic_loop.rs`/`tauri_commands.rs` edits.
- No global npm or user-installed Playwright dependency.

## Impact Targets

- GitNexus impact before editing:
  `BrowserProviderCapabilityCard`, `browser_provider_capability_cards`,
  `rank_browser_provider_candidates`, and file-level
  `src-tauri/src/browser/mod.rs` if exported.
- Stop on HIGH/CRITICAL risk.
- Run GitNexus staged `detect_changes` before commit.

## Implementation Steps

1. Add hosted-provider policy/capability DTOs and pure evaluator.
2. Extend the hosted capability-card metadata only as needed for cost/profile
   declaration, preserving safe local defaults.
3. Add tests for ADR Phase 10 gate cases and route fallback interaction.
4. Update tracker Quick View, branch hygiene, Phase 9E closure, and Phase 10A
   notes.

## Verification

Minimum focused commands:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::hosted_provider
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
rustfmt --edition 2021 --check src-tauri/src/browser/hosted_provider.rs src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs src-tauri/src/browser/provider_defaults.rs
git diff --check -- src-tauri/src/browser/hosted_provider.rs src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs src-tauri/src/browser/provider_defaults.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase10a-hosted-provider-contract.md
```

`src-tauri/src/browser/mod.rs` may still need the existing
`--config skip_children=true` rustfmt note if module-root formatting follows
legacy child files.

## Rollback

Revert the Phase 10A PR. Since this phase adds pure contracts and tests only,
rollback removes the hosted-provider evaluator and leaves Phase 9 recipe/domain
skill harnesses, Phase 8 provider routing/default policy, and local-first
provider execution untouched.

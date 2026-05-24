# Browser Runtime Phase 10B - Hosted Provider Harness Matrix

## Phase Goal

Close the ADR Phase 10 gate by adding a pure harness matrix for hosted-provider
escape hatches. The matrix must prove disabled fallback, data-boundary prompt,
artifact capture, cost visibility, local fallback, and opt-in mock-hosted ready
paths without adding a real hosted provider, SDK, network request, credential
storage, UI, IPC, DB migration, provider promotion, or live execution.

## ADR 11 Questions

1. User intent:
   Support rare browser tasks that need hosted isolation, proxying, scaling,
   hostile-site handling, CAPTCHA/manual takeover, or deployment escape hatches
   while keeping local providers as the default.

2. Autonomy level:
   The harness is L0/L1 evidence only. Any future hosted execution remains
   policy-gated and must not exceed the task's approved autonomy level.

3. Canonical truth source:
   uClaw provider policy, provider status, route decisions, and harness artifact
   refs remain canonical. Hosted vendor state is not a truth source.

4. TaskEvent entries:
   This phase emits no TaskEvents. It produces harness artifact data that future
   TaskEvents can cite when hosted-provider policy gates pass or block.

5. Context read and citation:
   The matrix reads `BrowserHostedProviderPolicy`,
   `BrowserRuntimeFeatureFlags`, hosted gate reports, and provider status. Any
   future model-visible hosted evidence must cite the attached harness artifact
   or provider-boundary action artifacts.

6. Capability cards:
   Consume the Phase 10A `browser.hosted` capability card and
   hosted-provider contract. No new provider card or provider promotion is
   introduced.

7. Policy hooks:
   Block when hosted providers are disabled, credentials are absent, the
   data-boundary prompt is not accepted for the task, artifact capture is
   missing, cost visibility is missing, profile storage is unsafe, hosted use
   case is unjustified, or local fallback is unavailable.

8. World projection:
   No UI projection changes in this phase. The harness report exposes the
   future projection facts: gate status, fallback provider, blockers,
   artifact/cost/data-boundary requirements, and provider readiness.

9. Harness cases:
   Default matrix cases cover hosted-provider disabled fallback,
   data-boundary prompt required, artifact capture required, cost visibility
   required, local-provider fallback required, and opt-in mock hosted ready.

10. Rollback or disable path:
   Revert this PR to remove the matrix. Hosted providers remain disabled by the
   `hosted_providers` feature flag, per-provider disabled ids, and local
   fallback policy.

11. Deliberately not owned:
   No hosted SDK/vendor integration, credential storage, network calls, live
   browser actions, UI/IPC/DB migration, default-provider mutation, task-event
   emission, `agentic_loop.rs`, or `tauri_commands.rs` changes.

## Allowed Files

- `src-tauri/src/harness/adapters/hosted_provider.rs`
- `src-tauri/src/harness/adapters/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase10b-hosted-provider-harness.md`

## Non-Goals

- No real hosted provider process, network call, SDK, API key, credential, or
  storage.
- No production provider execution path or provider route promotion.
- No UI, Tauri IPC, Settings, DB migration, TaskEvent emission, or
  `agentic_loop.rs`/`tauri_commands.rs` edits.
- No global npm or user-installed Playwright dependency.

## Impact Targets

- GitNexus impact before editing:
  file-level `src-tauri/src/harness/adapters/mod.rs`; new
  `hosted_provider.rs` has no existing symbol impact.
- Stop on HIGH/CRITICAL risk.
- Run GitNexus staged `detect_changes` before commit.

## Implementation Steps

1. Add a hosted-provider harness matrix adapter that composes Phase 10A's
   pure hosted gate and provider status conversion.
2. Add default cases for every ADR Phase 10 gate condition and an attachable
   JSON artifact report.
3. Add focused tests for matrix pass/fail coverage and artifact visibility.
4. Update tracker Quick View, branch hygiene, Phase 10A closure, and Phase 10B
   notes.

## Verification

Minimum focused commands:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::hosted_provider
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::hosted_provider
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
rustfmt --edition 2021 --check src-tauri/src/harness/adapters/hosted_provider.rs src-tauri/src/harness/adapters/mod.rs
git diff --check -- src-tauri/src/harness/adapters/hosted_provider.rs src-tauri/src/harness/adapters/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase10b-hosted-provider-harness.md
```

## Rollback

Revert the Phase 10B PR. Since this phase adds a pure harness adapter and tests
only, rollback removes the matrix artifact surface while leaving Phase 10A's
hosted-provider contract, Phase 9 recipe/domain-skill harnesses, and Phase 8
provider routing/default policy untouched.

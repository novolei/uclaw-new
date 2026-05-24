# Browser Runtime Phase 9E - Recipe Harness Matrix

## Scope

Phase 9E adds a pure recipe/domain-skill harness matrix contract that composes
the Phase 9A-9D recipe candidate, replay, locator cache, and domain-skill gate
decisions into an auditable report. It exists to prove the ADR Phase 9 gate
without replaying actions, persisting caches, writing domain-skill files, adding
UI/IPC/DB, or changing provider routing.

Allowed files:

- `src-tauri/src/browser/recipes.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase9e-harness-matrix.md`

Non-goals:

- no production recipe replay execution;
- no locator cache persistence;
- no domain-skill file generation or promotion side effects;
- no UI, IPC, Settings, DB migration, hosted provider, provider default, or
  runtime-pack behavior changes;
- no `agentic_loop.rs` or `tauri_commands.rs` edits.

## ADR Section 18 Questions

1. What user intent does this support?
   Repeated browser tasks should become faster and cheaper only after uClaw can
   prove that recipe/domain-skill candidates are safe, redacted, rollbackable,
   and invalidated on site/provider drift.

2. What autonomy level can it run at?
   This phase is model-free verification logic. It does not perform browser
   actions, so it can run inside low-autonomy harness/test contexts. Any future
   production replay remains policy-gated and outside this PR.

3. What is the canonical truth source?
   The canonical truth source is the recipe candidate data and the resulting
   harness matrix report. Provider state, generated files, and cache stores are
   deliberately not truth sources in this slice.

4. What TaskEvent entries does it emit?
   None. The phase produces pure DTOs and test reports only. Future runtime
   phases may map accepted reports to `browser_domain_skill_candidate_*` or
   recipe lifecycle events.

5. What context does it read, and how is it cited?
   It reads only in-memory `BrowserRecipeCandidate` values plus replay/locator
   requests derived from those candidates. Evidence remains cited through
   artifact refs and harness case ids already carried by the candidate.

6. What capability cards does it add or consume?
   It consumes provider id/version metadata already embedded in recipe keys. It
   does not add or mutate provider capability cards.

7. What policy hooks can block it?
   Production replay remains blocked by existing `production_replay_allowed`
   decisions. Redaction failures, fingerprint mismatch, provider-version
   mismatch, missing rollback, failed harness coverage, and missing domain-skill
   evidence all fail the matrix.

8. What world projection does the UI render?
   None in this phase. The matrix is backend contract/test evidence only. UI
   projection remains unchanged.

9. What harness cases prove it works?
   Focused Rust tests must cover replay success, fingerprint mismatch,
   provider-version invalidation, redaction rejection, promotion eligibility,
   rejection, rollback failure, and locator/domain-skill evidence.

10. What is the rollback or disable path?
    Revert this PR. Because no production route consumes the matrix and no
    persistence is added, rollback removes only additive DTO/helper/test code
    plus tracker/plan notes.

11. What does it deliberately not own?
    It does not own production replay, cache storage, domain-skill file writes,
    prompting, Settings/Startup Doctor UX, provider promotion, hosted provider
    choice, or agent-loop orchestration.

## Implementation Plan

1. Add matrix case/status/report DTOs in `recipes.rs`.
2. Add a pure `evaluate_browser_recipe_harness_matrix` helper that accepts a
   candidate, derives replay/locator/domain-skill gate decisions, and reports
   which ADR Phase 9 gate cases passed or failed.
3. Keep the helper deterministic: stable case ids, sorted/deduped blockers,
   normalized artifact and harness refs.
4. Add focused tests for all ADR gate dimensions and at least one all-green
   promoted candidate.
5. Update the Browser Runtime tracker with Phase 9D closure and Phase 9E
   branch hygiene, impact, verification, and next action.

## Impact Targets

- Primary code target: `src-tauri/src/browser/recipes.rs`
- Expected impact: LOW, additive pure functions/tests.
- Runtime impact: none; no live caller is added.

## Rollback

Revert the Phase 9E commit. No persisted user data, runtime files, provider
settings, or generated domain-skill files are touched.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
- `git diff --check -- src-tauri/src/browser/recipes.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase9e-harness-matrix.md`
- GitNexus detect_changes before commit.

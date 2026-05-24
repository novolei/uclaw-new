# Browser Runtime Phase 9A - Recipe Candidate Contract

## Summary

Phase 9A starts ADR Phase 9 with a pure Browser Recipe candidate contract. It
defines recipe keys, fingerprint/provider-version validation, redaction gates,
promotion readiness, and rollback metadata without replaying recipes or mutating
production behavior.

## ADR Section 18 Questions

1. **What user problem does this solve?** Repeated browser tasks should become
   faster only after successful provider behavior is observable and replay-safe.
2. **What autonomy level does it enable?** Candidate generation only. No
   production replay or self-promotion is enabled in this PR.
3. **What is the canonical truth source?** Browser task events, artifacts,
   harness evidence, provider cards, and the recipe candidate record.
4. **What TaskEvents does it emit?** None in Phase 9A. The contract names
   lifecycle states for future candidate/replay events.
5. **What context does it read, and how is it cited?** The pure contract
   consumes structured candidate inputs, artifact refs, harness case ids,
   fingerprint strings, provider id/version, and redaction review flags.
6. **What capability cards does it add or consume?** It records provider id and
   provider version in recipe keys, but does not consume live cards or alter
   provider ranking.
7. **What policy hooks can block it?** Redaction rejects secrets, private user
   data, task diaries, and transient pixel coordinates. Promotion readiness also
   requires harness evidence, artifact evidence, rollback id, fingerprint, and
   provider version.
8. **What world projection does the UI render?** None in this PR. Future phases
   may project candidate/replay status from this contract.
9. **What harness cases prove it works?** Focused Rust tests cover candidate
   acceptance, redaction rejection, fingerprint mismatch, provider-version
   invalidation, promotion readiness, rollback, and blocked production use.
10. **What is the rollback or disable path?** Revert this PR. No live cache,
    Settings, IPC, DB, replay, or provider route consumes the new module.
11. **What does it deliberately not own?** It does not own replay execution,
    production promotion, skill file generation, persistent storage, UI, IPC,
    DB migrations, provider selection, or live task-loop behavior.

## Allowed Files

- `src-tauri/src/browser/recipes.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase9a-recipe-contract.md`

## Non-Goals

- No recipe replay execution.
- No production mutation or automatic promotion.
- No domain-skill file writes.
- No locator cache persistence.
- No UI, IPC, Settings, DB migration, hosted provider, MCP, or provider route
  changes.
- No `agentic_loop.rs` or `tauri_commands.rs` edits.

## Impact Targets

- Add an isolated browser recipe contract module.
- Add one module export in `browser/mod.rs`.
- Keep Browser Runtime tracker current after PR #484 merge.

## Rollback

Revert the Phase 9A PR. Because the module is pure and unused by production
runtime code, rollback removes only the contract, tests, tracker, and plan.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
- `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/browser/mod.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes(scope=staged)`

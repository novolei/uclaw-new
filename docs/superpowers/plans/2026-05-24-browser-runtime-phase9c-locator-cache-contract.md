# Browser Runtime Phase 9C - Locator Cache Contract

## Summary

Phase 9C adds a pure locator-cache contract on top of the Phase 9A/9B recipe
candidate and normalization boundary. It defines when a recipe action locator is
eligible for deterministic reuse and when the runtime must fall back to
observation.

## ADR Section 18 Questions

1. **What user problem does this solve?** Repeated browser tasks need stable
   locator reuse decisions before replay can safely become faster.
2. **What autonomy level does it enable?** Locator reuse eligibility only. It
   does not execute replay, persist caches, or promote production behavior.
3. **What is the canonical truth source?** Recipe candidate keys, action
   templates, DOM/a11y fingerprint, provider id/version, validation counts,
   redaction state, promotion state, rollback id, and artifact refs.
4. **What TaskEvents does it emit?** None in this slice. Future runtime wiring
   may emit locator-cache hit/miss/reject events after this contract is proven.
5. **What context does it read, and how is it cited?** It reads explicit
   candidate/action inputs and cache lookup requests with artifact refs carried
   in the cache entry.
6. **What capability cards does it add or consume?** None. It records provider
   id/version in the locator cache key but does not change provider ranking.
7. **What policy hooks can block it?** Fingerprint mismatch, provider mismatch,
   transient coordinates, blank stable locators, redaction failures, replay
   failures, missing rollback, not-promoted state, and disabled production
   reuse all force observation fallback.
8. **What world projection does the UI render?** None in this PR. Future UI may
   project cache readiness from this pure contract.
9. **What harness cases prove it works?** Focused Rust tests cover cache entry
   build, reusable promoted locator, fingerprint/provider mismatch, redaction
   rejection, validation failure rejection, blank locator rejection, and
   not-promoted fallback.
10. **What is the rollback or disable path?** Revert this PR. No live runtime
    path consumes the new contract.
11. **What does it deliberately not own?** It does not own replay execution,
    locator persistence, domain-skill file generation, production promotion,
    UI, IPC, DB migrations, provider selection, task-loop wiring, or hosted
    providers.

## Allowed Files

- `src-tauri/src/browser/recipes.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase9c-locator-cache-contract.md`

## Non-Goals

- No recipe replay execution.
- No locator cache persistence.
- No domain-skill file writes.
- No production promotion or provider default changes.
- No UI, IPC, Settings, DB migration, hosted provider, MCP, or task-loop
  changes.
- No `agentic_loop.rs` or `tauri_commands.rs` edits.

## Impact Targets

- Add pure locator-cache key/entry/validation/reuse decision DTOs.
- Add a deterministic builder from a recipe candidate action to a locator-cache
  entry.
- Keep Phase 9A replay validation and Phase 9B normalization behavior unchanged
  except for focused tests that exercise the new locator-cache boundary.
- Update the Browser Runtime tracker to close Phase 9B and open Phase 9C.

## Rollback

Revert the Phase 9C PR. Phase 9A candidate/replay validation and Phase 9B
normalization remain available and no runtime path consumes the new locator
cache helper.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes(scope=staged)`

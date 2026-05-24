# Browser Runtime Phase 9B - Recipe Normalization Intake

## Summary

Phase 9B adds the pure intake boundary that turns successful browser action
evidence into deterministic recipe candidates. It builds on Phase 9A's
candidate/replay contract without replaying recipes, persisting caches, writing
domain skills, or changing live provider behavior.

## ADR Section 18 Questions

1. **What user problem does this solve?** Successful browser actions need a
   deterministic, redacted candidate form before repeated tasks can become
   cheaper.
2. **What autonomy level does it enable?** Candidate creation only. No
   production replay, persistence, or self-promotion is enabled.
3. **What is the canonical truth source?** Structured browser action evidence,
   artifact refs, provider id/version, DOM/a11y fingerprint, and redaction
   review metadata.
4. **What TaskEvents does it emit?** None in this slice. Future wiring may emit
   candidate lifecycle events after this pure intake boundary is proven.
5. **What context does it read, and how is it cited?** It reads explicit
   normalization inputs: recipe key, action observations, artifact refs, harness
   case ids, redaction report, and rollback id.
6. **What capability cards does it add or consume?** It records provider id and
   provider version from the recipe key, but does not consume live provider cards
   or alter provider ranking.
7. **What policy hooks can block it?** Failed action observations, missing
   evidence, redaction failures, transient coordinates, blank locators, missing
   rollback, and replay failures keep candidates rejected or not promotion-ready.
8. **What world projection does the UI render?** None in this PR. Future UI may
   project candidate normalization status from this contract.
9. **What harness cases prove it works?** Focused Rust tests cover successful
   normalization, failed-action rejection, artifact aggregation, redaction
   rejection through Phase 9A validation, and transient coordinate rejection.
10. **What is the rollback or disable path?** Revert this PR. The new code is
    pure and unused by production runtime paths.
11. **What does it deliberately not own?** It does not own replay execution,
    locator cache persistence, domain-skill file generation, production
    promotion, UI, IPC, DB migrations, provider selection, or live task-loop
    wiring.

## Allowed Files

- `src-tauri/src/browser/recipes.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase9b-recipe-normalization.md`

## Non-Goals

- No recipe replay execution.
- No locator cache persistence.
- No domain-skill file writes.
- No production promotion or provider default changes.
- No UI, IPC, Settings, DB migration, hosted provider, MCP, or task-loop
  changes.
- No `agentic_loop.rs` or `tauri_commands.rs` edits.

## Impact Targets

- Add pure recipe normalization input/output DTOs.
- Add a deterministic builder from successful action evidence to
  `BrowserRecipeCandidate`.
- Keep Phase 9A replay validation unchanged except for tests that exercise the
  new normalization boundary.
- Update the Browser Runtime tracker to close Phase 9A and open Phase 9B.

## Rollback

Revert the Phase 9B PR. Phase 9A candidate/replay validation remains available
and no runtime path consumes the new normalization helper.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes(scope=staged)`

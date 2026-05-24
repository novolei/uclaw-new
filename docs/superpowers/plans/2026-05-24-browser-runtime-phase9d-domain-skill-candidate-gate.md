# Browser Runtime Phase 9D - Domain Skill Candidate Gate

## Summary

Phase 9D adds a pure domain-skill candidate gate on top of the recipe candidate,
normalization, and locator-cache contracts. It validates whether a redacted
domain-skill candidate has enough run evidence, harness coverage, rollback
metadata, and promotion state to be eligible for future file generation.

## ADR Section 18 Questions

1. **What user problem does this solve?** uClaw needs repeatable browser-site
   knowledge without silently storing private data or promoting brittle
   playbooks.
2. **What autonomy level does it enable?** Candidate eligibility only. No
   domain-skill file is written or promoted in production.
3. **What is the canonical truth source?** Recipe candidate metadata, the
   embedded `BrowserDomainSkillCandidate`, redaction report, harness case ids,
   artifact refs, provider id/version, promotion state, and rollback id.
4. **What TaskEvents does it emit?** None in this slice. Future wiring may emit
   `browser_domain_skill_candidate_created/promoted/rejected` after this pure
   gate is proven.
5. **What context does it read, and how is it cited?** It reads explicit recipe
   candidate fields and keeps artifact/harness refs as evidence in the gate
   report.
6. **What capability cards does it add or consume?** None. Provider id/version
   remain evidence fields only.
7. **What policy hooks can block it?** Redaction failures, missing stable URL
   patterns, missing selector/wait/domain evidence, private API shapes without
   auth-boundary notes, missing artifacts, missing harness coverage, failed
   replays, missing rollback, and non-promoted state block eligibility.
8. **What world projection does the UI render?** None in this PR. Future UI may
   render candidate status from this contract.
9. **What harness cases prove it works?** Focused Rust tests cover eligible
   domain-skill candidates, missing candidate rejection, redaction rejection,
   missing evidence rejection, private API auth-boundary rejection, and
   non-promoted fallback.
10. **What is the rollback or disable path?** Revert this PR. No live runtime
    path consumes the new gate.
11. **What does it deliberately not own?** It does not own domain-skill file
    generation, production promotion, replay execution, locator persistence,
    UI, IPC, DB migrations, provider selection, task-loop wiring, or hosted
    providers.

## Allowed Files

- `src-tauri/src/browser/recipes.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase9d-domain-skill-candidate-gate.md`

## Non-Goals

- No domain-skill file writes.
- No recipe replay execution.
- No locator cache persistence.
- No production promotion or provider default changes.
- No UI, IPC, Settings, DB migration, hosted provider, MCP, or task-loop
  changes.
- No `agentic_loop.rs` or `tauri_commands.rs` edits.

## Impact Targets

- Add pure domain-skill candidate gate status/report DTOs.
- Add a deterministic validator from `BrowserRecipeCandidate` to a
  domain-skill candidate report.
- Keep Phase 9A replay validation, Phase 9B normalization, and Phase 9C locator
  cache behavior unchanged except for focused tests that exercise the new
  domain-skill boundary.
- Update the Browser Runtime tracker to close Phase 9C and open Phase 9D.

## Rollback

Revert the Phase 9D PR. Phase 9A/9B/9C recipe contracts remain available and no
runtime path consumes the new domain-skill gate.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes(scope=staged)`

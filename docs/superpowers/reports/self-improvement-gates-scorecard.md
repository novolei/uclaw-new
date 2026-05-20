# Self-Improvement Gates Scorecard

**Date:** 2026-05-20
**Branch:** `codex/overnight-harness-248-252`
**Scope:** PR #252, self-improvement promotion gates for memory, gbrain, skills, prompts, and hooks.

## Goal

Prevent agent self-improvement writes from being promoted just because they look useful in one session. A candidate must carry explicit evidence, pass required harness suites, meet a score threshold, have no blocking regressions, and include a rollback reference before promotion.

## Implemented Surface

- `src-tauri/src/harness/self_improvement.rs`
  - Candidate model for `memory`, `gbrain`, `skill`, `prompt`, and `hook`.
  - Gate policy with minimum average score, required suites, and rollback requirements.
  - Deterministic verdicts: `promote`, `hold`, `reject`.
  - Fixture suite covering a promotable memory candidate and a rejected unsafe skill candidate.
- `run_self_improvement_gate_harness`
  - App-visible Tauri command returning gate reports for the current fixture suite.

## Gate Checks

| Check | Purpose |
|---|---|
| `has_evidence` | Candidate cannot be promoted without harness evidence. |
| `score_threshold` | Average evidence score must meet policy threshold. |
| `required_suite:*` | Required suites must be present and passing. |
| `no_blockers` | Any blocker turns the candidate into a rejection. |
| `rollback_ref` | Mutable candidates must be reversible. |

## Fixture Outcomes

| Candidate | Kind | Expected Verdict | Reason |
|---|---|---|---|
| `candidate.memory.safe_profile_fact` | memory | promote | Memory/gbrain and agent control-plane evidence both pass, score is above threshold, rollback exists. |
| `candidate.skill.unsafe_shell` | skill | reject | Evidence contains a permission-boundary regression blocker. |

## Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::self_improvement --lib
cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw
git diff --check
```

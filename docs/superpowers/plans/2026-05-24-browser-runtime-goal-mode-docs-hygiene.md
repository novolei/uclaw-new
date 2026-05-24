# Goal-Mode Docs Hygiene

## Summary

This docs-only slice removes behavior-spec friction that can derail long-running
goal-mode phase chains. It aligns `AGENTS.md`, `BEHAVIOR.md`, and `CONTEXT.md`
with the Browser Runtime phase-pack workflow while preserving the underlying
safety contract: isolated worktrees, one plan per PR, GitNexus verification,
fresh review for genuinely broad/risky work, and no user worktree clobbering.
It also removes the special DMZ treatment for `agentic_loop.rs` and
`tauri_commands.rs`; those runtime hot paths should use normal code-change
discipline instead of requiring permission-only PRs. Because both files are
already large, the new rule also discourages adding large business-logic blocks
there and favors focused modules with thin orchestration/IPC shims.

## ADR Section 18 Questions

1. **User intent:** keep autonomous goal-mode implementation moving without
   obsolete branch templates or over-broad stop rules creating false blockers.
2. **Autonomy level:** L0 documentation policy only; no runtime behavior changes.
3. **Canonical truth source:** `BEHAVIOR.md` remains the behavior contract;
   `AGENTS.md` mirrors critical Codex-facing rules; `CONTEXT.md` stays
   reference material.
4. **TaskEvent entries:** none.
5. **Context read/citation:** read current `AGENTS.md`, `BEHAVIOR.md`,
   `CONTEXT.md`, and Browser Runtime tracker state.
6. **Capability cards:** none.
7. **Policy hooks:** clarify docs-only GitNexus impact expectations, remove
   automatic DMZ gates for `agentic_loop.rs`/`tauri_commands.rs`, and preserve
   reviewer gates for broad/risky/HIGH-impact work. Add a thin-file direction
   so these large hot-path files stop accumulating unrelated logic.
8. **World projection:** none.
9. **Harness cases:** markdown grep/diff checks plus GitNexus detect.
10. **Rollback/disable path:** revert this docs PR.
11. **Does not own:** no code, no runtime packs, no IPC, no provider promotion,
   no migrations, no behavior-hook implementation.

## Allowed Files

- `AGENTS.md`
- `BEHAVIOR.md`
- `CONTEXT.md`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-goal-mode-docs-hygiene.md`

## Non-Goals

- No Rust, TypeScript, Tauri, DB migration, IPC, or runtime-pack changes.
- No removal of GitNexus requirements.
- No weakening of GitNexus, verification, or PR review discipline; this removes
  obsolete file-name-only DMZ gates that caused false blockers.
- No cleanup of the auto-managed GitNexus stats block unless it changes during
  verification.

## Follow-Up Gate Before Phase 5B

Before resuming Phase 5B implementation, audit merged Phase 1 through Phase 5A
for target/design drift caused by the old `agentic_loop.rs` and
`tauri_commands.rs` constraints. The audit must explicitly inspect:

- whether any code stayed in dry-run lanes because real integration through
  `agentic_loop.rs` or `tauri_commands.rs` was avoided for policy reasons
  rather than ADR sequencing;
- Phase 2 runtime-pack executor boundaries and whether dry-run-only behavior is
  still intentional;
- Phase 4X Settings action dry-run IPC and whether it now needs a real
  execution follow-up before later provider phases;
- Phase 4O-4P prompt dispatch decisions, where `agentic_loop.rs` was avoided;
- Phase 4R-4X Settings/IPC decisions, where `tauri_commands.rs` was avoided;
- Phase 5A provider contract and whether the next slice should leave the
  contract lane or introduce supervised child-worker execution;
- whether any workarounds should be replaced by thinner feature modules plus
  normal hot-path shims before later provider execution phases.

If drift exists, add a corrective phase/plan and finish it before continuing the
rest of the Browser Runtime roadmap.

## Verification

- `rg -n "prep/codex-absorption|<M\\*-T\\*>|requires a writer/reviewer|tauri_commands\\.rs.*special DMZ|agentic_loop\\.rs.*special DMZ" AGENTS.md BEHAVIOR.md CONTEXT.md`
- `git diff --check -- AGENTS.md BEHAVIOR.md CONTEXT.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-goal-mode-docs-hygiene.md`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the docs hygiene commit. No runtime state, database rows, browser
sessions, provider selection, or user data are changed.

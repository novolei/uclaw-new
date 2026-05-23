# Browser Runtime Phase 3F - Root Route Reviewer Plan

## Context

Phase 3E merged Startup Splash recovery surfaces through PR #423. Phase 3
still contains the ADR requirement to replace the generic initialization route
with the branded startup experience, but the Phase 3D attempt stopped because
final staged GitNexus detect reported HIGH risk for `App`.

This docs-only control slice keeps the tracker accurate and records the review
gate before any future root `App` integration.

## Scope

- Mark Phase 3E as merged in the Browser Runtime tracker.
- Record PR #423, commit `52035cf4`, and merge commit `f2dabbe3`.
- Add a Phase 3F blocker section that explains why root route integration must
  wait for an explicit HIGH-risk reviewer plan.
- Keep Phase 4 gated until the branded shell route is reviewed and landed.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3f-root-route-reviewer-plan.md`

## Non-Goals

- No source code changes.
- No root `App`, `main.tsx`, AppShell, IPC, runtime-pack, provider, Settings,
  DB migration, TaskEvent, Playwright, or MCP changes.
- No attempt to reduce or bypass GitNexus HIGH risk.
- No network downloads, runtime installation, cleanup, rollback, or user-data
  deletion.

## ADR Section 18 Questions

1. **What user-facing problem is solved?**
   The tracker stops implying Phase 3E is still in progress and clearly tells
   the next session why root startup routing is blocked.
2. **What runtime owns the behavior?**
   No runtime behavior changes in this slice. Future root route work remains
   owned by the Rust Browser Runtime Supervisor and the frontend startup shell.
3. **What is the local-first path?**
   Docs only. The recorded future path preserves local Startup Splash,
   Startup Doctor, and uClaw-managed runtime-pack boundaries.
4. **What policy gate applies?**
   The gate is GitNexus HIGH-risk review: no root `App` edit proceeds without
   an explicit reviewer plan and blast-radius acceptance.
5. **What artifacts are visible?**
   The tracker records PR #423, commit `52035cf4`, merge commit `f2dabbe3`,
   and the Phase 3D HIGH-risk affected processes.
6. **What is the rollback story?**
   Revert this docs-only commit to remove the Phase 3F control note.
7. **What is out of scope?**
   All code, UI behavior, IPC, DB, provider, and runtime side effects.
8. **How does this avoid surprising users?**
   It prevents a broad root-shell change from landing under a lower-risk phase
   label and keeps future reviewers pointed at the actual blast radius.
9. **How will it be verified?**
   `git diff --check`, `git diff --cached --check`, and GitNexus
   `detect_changes(scope=staged)` should report docs-only low/none risk.
10. **What metrics or signals matter?**
    Tracker accuracy, PR/commit continuity, and no affected execution flows.
11. **What remains after this slice?**
    A future Phase 3G or reviewer-approved Phase 3D retry must define the
    root `App` integration owner/reviewer path before Phase 4 can start.

## Impact Targets

No code symbols are edited. GitNexus impact is not required for docs-only
changes, but final staged `detect_changes` is required before commit.

## Verification

- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase3f-root-route-reviewer-plan.md`
- `git diff --cached --check`
- GitNexus `detect_changes(scope=staged, repo=phase3f worktree)`

## Rollback

Revert the Phase 3F docs-only commit.

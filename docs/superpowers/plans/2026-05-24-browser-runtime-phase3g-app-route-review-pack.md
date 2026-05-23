# Browser Runtime Phase 3G - Root App Route Review Pack

## Context

Phase 3D attempted to replace the root `App` loading spinner with the branded
Startup Splash. Focused tests passed, but final staged GitNexus detect reported
HIGH risk because `App` participates in 9 top-level execution processes:
`App -> MakeListener`, `App -> UpdateState`, `App -> Reg`,
`App -> CreateInitialStreamState`, `App -> BuildResolvedTarget`,
`App -> UpsertBrowserTaskStep`, `App -> SafeU`, `App -> GetSettings`, and
`App -> GetCachedStickyUserMessage`.

Phase 3F recorded that gate and Phase 4 now remains blocked until the root
startup route is accepted through the writer/reviewer flow required by
`BEHAVIOR.md` section 8.

This Phase 3G slice is the acceptance pack for that reviewer flow. It does not
implement the root route.

## Scope

- Close Phase 3F as merged through PR #424.
- Define the exact writer branch scope for a future root `App` startup route.
- Define the fresh reviewer prompt and evidence bundle.
- Define go/no-go gates for accepting the HIGH-risk blast radius.
- Keep Phase 4 blocked until the reviewer flow lands the root route PR.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3g-app-route-review-pack.md`

## Non-Goals

- No source code changes.
- No root `App`, `main.tsx`, AppShell, Startup Splash, IPC, runtime-pack,
  provider, Settings, DB migration, TaskEvent, Playwright, or MCP changes.
- No approval to bypass GitNexus HIGH risk.
- No network downloads, runtime installation, cleanup, rollback, or user-data
  deletion.

## ADR Section 18 Questions

1. **What user-facing problem is solved?**
   Users cannot get the branded Startup Splash as the first app route until the
   root `App` change is reviewed. This pack makes the review path concrete
   instead of leaving Phase 3 stuck behind an implicit warning.
2. **What runtime owns the behavior?**
   No runtime behavior changes here. The future implementation remains a
   frontend startup-shell route over Rust-owned Browser Runtime Supervisor /
   runtime-pack status.
3. **What is the local-first path?**
   The future route must render existing local Startup Splash components and
   local Startup Doctor view models only. It must not depend on network,
   global npm, external Playwright installs, or remote providers.
4. **What policy gate applies?**
   GitNexus HIGH requires writer/reviewer acceptance. The writer may prepare a
   narrow PR; the reviewer must inspect only the diff, GitNexus context for
   `App`, and verification output before approving or rejecting.
5. **What artifacts are visible?**
   This pack records the affected processes, future branch naming, reviewer
   prompt, expected tests, browser-smoke caveat, rollback, and next action.
6. **What is the rollback story?**
   Revert this docs-only commit. For the future route PR, rollback must be a
   single revert restoring the generic initialization spinner.
7. **What is out of scope?**
   Phase 4 Settings, browser runtime controls, task-time preparation UX,
   runtime side effects, IPC, provider promotion, Playwright launch, and
   screenshot expansion beyond the root-route acceptance gates.
8. **How does this avoid surprising users?**
   It prevents a root lifecycle change from landing without naming the exact
   initialization processes at risk and without a fresh reviewer.
9. **How will it be verified?**
   This docs slice uses whitespace checks and GitNexus staged detect. The
   future route PR must run focused App/Startup tests, preview screenshots,
   root app smoke with the known dev-mock caveat, default Browser Runtime Rust
   regressions, diff checks, and GitNexus staged detect.
10. **What metrics or signals matter?**
    No affected processes for this pack; for the future route, reviewer
    acceptance of the HIGH blast radius, unchanged listener registration count,
    no new root console errors before handoff, and preserved AppShell handoff.
11. **What remains after this slice?**
    A future Phase 3H writer PR may implement the root `App` startup route only
    after the DRI/user explicitly accepts this reviewer pack.

## Future Writer Scope

Recommended branch:
`codex/browser-runtime-phase3h-app-startup-route`

Allowed files for the future writer PR:

- `ui/src/App.tsx`
- `ui/src/App.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3h-app-startup-route.md`

Optional only if the focused tests prove it is necessary:

- `ui/src/test-utils/render.tsx`

Explicit non-goals for the writer PR:

- no `main.tsx` root error boundary changes;
- no AppShell overlay/prompt changes;
- no backend IPC or Tauri command changes;
- no runtime-pack Rust changes;
- no Settings UI;
- no DB migrations;
- no TaskEvent emission;
- no Playwright launch or provider selection.

## Reviewer Prompt

Use a fresh session or separate IDE. Do not read the writer transcript. Start
from:

```text
Review PR <number> for uClaw Browser Runtime Phase 3H root App startup route.

Read only:
1. gh pr diff <number>
2. docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md §12 Phase 3
3. docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md Phase 3D/3G/3H notes
4. GitNexus context for App in ui/src/App.tsx

Focus on whether replacing the root loading spinner with StartupSplash changes
listener registration, settings/model initialization, AppShell handoff, or root
error behavior. Report blockers before style feedback.
```

## Future Go / No-Go Gates

Go only if all are true:

- The writer PR edits only the allowed files or justifies an explicit allowed
  file expansion in the tracker before implementation.
- Pre-edit GitNexus impact for `App` is reported in the plan and PR.
- Final GitNexus staged detect is HIGH only for the already-known `App`
  processes, with no new affected process names beyond the Phase 3D list.
- Focused App/Startup tests pass.
- Browser preview screenshots for first-frame, details, offline recovery, and
  failed recovery remain console-clean.
- Root app smoke either reaches AppShell or reproduces only the already-known
  `WelcomeView.tsx` null `.filter` dev-mock failure after startup handoff.
- Default Browser Runtime Rust regressions pass.
- Reviewer explicitly accepts the HIGH-risk blast radius in PR review.

No-go if any are true:

- New affected processes appear beyond the Phase 3D list.
- Listener registration, settings/model initialization, or AppShell handoff
  changes without a separate plan.
- The future PR touches DMZ files.
- Browser startup becomes an indefinite trap instead of entering main UI after
  the time budget.
- Verification relies only on preview harness tests and does not exercise the
  root `App` branch.

## Verification For This Slice

- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase3g-app-route-review-pack.md`
- `git diff --cached --check`
- GitNexus `detect_changes(scope=staged, repo=phase3g worktree)`

## Rollback

Revert this docs-only commit to remove the reviewer pack and restore the
previous tracker state.

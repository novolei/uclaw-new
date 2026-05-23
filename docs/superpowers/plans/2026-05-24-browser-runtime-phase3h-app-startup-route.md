# Browser Runtime Phase 3H - App Startup Route

## Context

Phase 3G merged the writer/reviewer acceptance pack for the HIGH-risk root
`App` startup route. ADR Phase 3 still requires the main Tauri WebView first
route to use the branded Startup Splash instead of a generic initialization
spinner. This writer slice implements that narrow root loading-branch swap and
leaves reviewer acceptance to the PR review flow.

## Scope

- Replace the root `App` loading spinner with the existing `StartupSplash`.
- Preserve the existing initialization sequence:
  `getSettings` -> language cache -> `initializeUiPreferences` ->
  `getActiveModel` -> `AppShell` handoff.
- Add focused `App` tests for splash-first rendering and post-initialization
  handoff.
- Update the Browser Runtime tracker with Phase 3H plan, impact, and
  verification notes.

## Allowed Files

- `ui/src/App.tsx`
- `ui/src/App.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3h-app-startup-route.md`

## Non-Goals

- No `main.tsx` root error boundary changes.
- No AppShell overlay, prompt, listener, or navigation changes.
- No backend IPC, Tauri command, runtime-pack Rust, provider, Settings, DB
  migration, TaskEvent, Playwright, MCP, or runtime side-effect changes.
- No attempt to fix the pre-existing browser-smoke `WelcomeView.tsx` null
  `.filter` dev-mock issue.
- No Phase 4 Settings UX.

## ADR Section 18 Questions

1. **What user-facing problem is solved?**
   Startup no longer shows an anonymous spinner; it uses the branded
   Startup Splash as the main WebView first route.
2. **What runtime owns the behavior?**
   The frontend `App` root owns only the loading branch. Runtime readiness
   remains owned by the Rust Browser Runtime Supervisor / runtime-pack status
   and existing Startup Doctor view models.
3. **What is the local-first path?**
   The loading route uses the bundled React `StartupSplash` component and local
   CSS only. It does not require network, Playwright, or global npm.
4. **What policy gate applies?**
   This is the writer half of the HIGH-risk writer/reviewer flow from Phase 3G.
   The PR must be reviewed by a fresh reviewer before merge.
5. **What artifacts are visible?**
   The tracker records pre-edit GitNexus impact, final staged detect, tests,
   screenshots/smoke results, commit, and PR.
6. **What is the rollback story?**
   Revert the single Phase 3H commit to restore the generic spinner branch.
7. **What is out of scope?**
   AppShell behavior, global listeners, root error boundary, backend runtime
   behavior, Settings, provider promotion, and task-time preparation UX.
8. **How does this avoid surprising users?**
   It changes only the loading visual while preserving the same async
   initialization and main-app handoff.
9. **How will it be verified?**
   Focused App/Startup Vitest tests, browser preview checks, root app smoke
   with the known dev-mock caveat, default Browser Runtime Rust regressions,
   whitespace checks, and GitNexus staged detect.
10. **What metrics or signals matter?**
    The loading branch renders Startup Splash before `getSettings` resolves;
    AppShell renders after initialization; final detect has no new processes
    beyond the Phase 3D list if HIGH appears.
11. **What remains after this slice?**
    Fresh reviewer acceptance of the PR. After merge, Phase 4 can begin with
    Settings/task-time UX slices.

## Impact Targets

- `App` in `ui/src/App.tsx`.

Pre-edit GitNexus impact:

- `App` upstream impact in the Phase 3H worktree reported LOW risk, 0 direct
  callers, 0 affected processes, and 0 affected modules.

## Verification

- `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx src/lib/startup/startup-doctor.test.ts`
- Browser preview screenshots for first-frame/details/offline/failed scenarios.
- Root app smoke under `VITE_UCLAW_MOCK_TAURI=1`; acceptable result is AppShell
  or only the known post-handoff `WelcomeView.tsx` null `.filter` dev-mock
  issue.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable if
  no Rust files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes(scope=staged, repo=phase3h worktree)`

## Rollback

Revert the Phase 3H commit to restore the previous generic spinner loading
branch and remove the focused App tests/tracker updates.

# Browser Runtime Phase 3E - Startup Recovery Surfaces

## Scope

Phase 3E adds branded, side-effect-free recovery surfaces to the existing
Startup Splash for degraded and failed startup states. It also records the
Phase 3D `App` route stop gate in the tracker so future sessions do not retry
the same high-risk root edit blindly.

- Recovery panel for degraded and failed Startup Doctor phases;
- deterministic preview coverage for deferred/offline and failed runtime setup
  recovery states;
- focused Startup Splash tests for recovery copy and diagnostics expansion;
- tracker update marking Phase 3C merged, Phase 3D blocked, and Phase 3E
  current.

## ADR Section 18 Answers

1. **User intent:** startup failures and deferred browser-runtime setup should
   feel recoverable and calm instead of exposing raw diagnostic noise first.
2. **Autonomy level:** L0. This slice renders recovery guidance only; no repair,
   download, rollback, cleanup, or provider action is executed.
3. **Canonical truth source:** existing Startup Doctor view model remains the
   UI truth; recovery surfaces derive from its phase and check details.
4. **TaskEvent entries:** no TaskEvents are emitted.
5. **Context read/cited:** reads ADR Phase 3 recovery-surface requirement and
   tracker state after PR #422 plus the Phase 3D GitNexus HIGH stop.
6. **Capability cards:** no provider capability changes.
7. **Policy hooks:** no runtime action is attempted; backend policy gates remain
   untouched.
8. **World projection:** recovery copy mirrors future projection states without
   writing projection data yet.
9. **Harness cases:** Startup Splash tests and preview scenarios cover deferred
   and failed recovery states; browser screenshots can target the existing
   preview page.
10. **Rollback/disable path:** revert the Startup Splash recovery panel,
    preview scenario/test updates, this plan, and tracker update.
11. **Does not own:** root `App` routing, backend IPC, runtime execution,
    downloads, cleanup, rollback, Settings UI, DB migrations, TaskEvents,
    provider promotion, or final asset production.

## Allowed Files

- `ui/src/components/startup/StartupSplash.tsx`
- `ui/src/components/startup/StartupSplash.test.tsx`
- `ui/src/components/startup/startup-splash-scenarios.ts`
- `ui/src/components/startup/startup-splash-scenarios.test.ts`
- `ui/startup-splash-preview.html`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3e-startup-recovery-surfaces.md`

## Non-Goals

- No `App`, `main.tsx`, Tauri command, Settings UI, DB migration, runtime-pack
  mutation, Playwright launch, provider promotion, or TaskEvent emission.
- No real recovery buttons that invoke backend actions.
- No final canonical splash artwork in this slice.

## Impact Targets

- `StartupSplash` in `ui/src/components/startup/StartupSplash.tsx`.
- `getStartupSplashScenario` in
  `ui/src/components/startup/startup-splash-scenarios.ts`.
- Standalone preview HTML favicon metadata.
- GitNexus impact before editing:
  - `StartupSplash`: LOW, 1 direct caller (`startup-splash-preview.tsx`), 0
    affected processes;
  - `getStartupSplashScenario`: LOW, preview-only Startup module impact, 0
    affected processes.

## First Tests

- `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx src/components/startup/startup-splash-scenarios.test.ts src/lib/startup/startup-doctor.test.ts`
- Browser preview checks against deferred and failed scenarios if the focused
  UI tests pass.
- `git diff --check -- <changed-files>`

The broader phase closeout should also run browser-runtime Rust regressions:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`

## Rollback

Revert the Startup Splash recovery panel, preview scenario/test updates, this
plan, and the tracker update. No persisted user data, runtime files, DB rows,
browser profiles, or provider state are created.

## Expected Verification Output

- Focused Startup Splash and scenario tests pass.
- Deferred and failed preview scenarios expose recovery guidance without
  emitting side effects.
- Rust browser-runtime regressions pass with no Rust source changes.
- `rustfmt` is not applicable because no Rust files change.
- `git diff --check` returns no output.
- GitNexus detect-changes reports low risk and no unexpected execution flows.

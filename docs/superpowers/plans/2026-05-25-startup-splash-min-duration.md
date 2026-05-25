# Startup Splash Minimum Visibility Plan

## 1. Intent

Manual Browser Runtime verification showed the production Startup Splash can
flash by too quickly to communicate any state. This slice makes the splash
perceptible during fast startup while preserving the existing initialization
flow.

## 2. Autonomy

The frontend owns the presentation gate only. Backend readiness, Browser
Runtime status, and app initialization remain authoritative through existing
IPC calls.

## 3. Truth Source

`App` continues to load settings, UI preferences, and the active model from the
existing Tauri bridge. `StartupSplash` continues to read Browser Runtime status
through `getBrowserRuntimeStatus`.

## 4. TaskEvent

No new TaskEvents. This is a shell UX timing improvement, not a runtime event
or Browser task behavior change.

## 5. Context

The change is scoped to the app startup route and its test. It should not touch
agent runtime, task checkpoints, provider selection, Browser Runtime execution,
identity, or harness code.

## 6. Capability

No new capability is introduced. The existing Startup Splash surface becomes
visible long enough for users to perceive the brand, readiness state, progress,
and diagnostics affordance.

## 7. Hooks

Use React timers inside `App` to keep the splash mounted for a minimum visible
duration and then fade it out before mounting `AppShell`.

## 8. Projection

No projection model changes. The visible projection remains the existing
Startup Doctor view model.

## 9. Harness

Focused Vitest coverage must prove:

- pending initialization still renders Startup Splash;
- completed initialization does not immediately render AppShell;
- after the minimum visible duration and exit transition, AppShell renders and
  initialization side effects still happened.

## 10. Rollback

Revert this plan, the `App` timing gate, the focused test updates, and the
tracker note. The app will return to immediate AppShell handoff after
initialization.

## 11. Non-Goals

- No redesign of the Startup Splash visual system.
- No backend IPC changes.
- No runtime-pack execution changes.
- No provider promotion or Browser task routing changes.
- No DB migration.

## Allowed Files

- `ui/src/App.tsx`
- `ui/src/App.test.tsx`
- `docs/superpowers/plans/2026-05-25-startup-splash-min-duration.md`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`

## Verification

Run:

```bash
cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx
git diff --check -- ui/src/App.tsx ui/src/App.test.tsx docs/superpowers/plans/2026-05-25-startup-splash-min-duration.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
```

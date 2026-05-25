# Startup Splash If2Ai Visual Port Plan

## Scope

Port the splash screen visual language from `/Users/ryanliu/Documents/IfAI/if2Ai` into uClaw while preserving uClaw's current startup UX logic from PR #501.

Allowed files:
- `ui/src/components/startup/StartupSplash.tsx`
- `ui/src/components/startup/StartupSplash.test.tsx`
- `ui/src/components/startup/startup-splash-scenarios.test.ts`
- `ui/src/styles/globals.css`
- `ui/src/assets/startup/uclaw-icon.png`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan

Non-goals:
- No changes to `App.tsx`, root startup routing, or minimum splash duration.
- No backend, IPC, database migration, provider, runtime-pack, or browser task behavior.
- No global npm or Playwright install path changes.
- No new browser provider promotion, runtime side effects, or TaskEvent schema work.

## ADR Section 18 Answers

1. User intent: make startup feel intentional and legible by showing a real splash screen long enough to communicate uClaw is preparing local runtime/workspace state.
2. Autonomy level: L0 UI-only presentation; it does not authorize or run browser automation.
3. Canonical truth source: the existing `StartupDoctorViewModel` and live `getBrowserRuntimeStatus()` adapter remain canonical for splash diagnostics.
4. TaskEvent entries: none added in this PR; existing Startup Doctor event semantics remain unchanged.
5. Context read and citations: reads only the current startup doctor view model and browser-runtime status report already consumed by `StartupSplash`.
6. Capability cards: none added or consumed beyond the existing Browser Runtime readiness surface.
7. Policy hooks: none changed; runtime preparation, provider use, identity, and browser actions remain behind existing policy gates.
8. World projection rendered: splash continues to render runtime readiness, progress, degraded/failed recovery text, and settings deep-link affordance; visual treatment is ported from If2Ai.
9. Harness cases: focused Vitest coverage for default frame, live status load, recovery guidance, details expansion, settings link, preview scenarios, plus browser screenshot smoke.
10. Rollback/disable path: revert this PR to restore the previous uClaw splash visuals while leaving PR #501's minimum-duration behavior intact.
11. Deliberately not owned: app boot orchestration, browser provider execution, runtime pack installation/repair/rollback behavior, settings internals, and broader app theming.

## Implementation Notes

- Recreate the If2Ai splash structure: warm sand background, drifting grid, centered icon, typed/glitch wordmark, wave-dot loader, and small build/version telemetry.
- Adapt copy and identity to uClaw: project name `uClaw`, stage text from `StartupDoctorViewModel.statusLine`, app icon from uClaw's bundled icon.
- Keep diagnostic details accessible and visible for degraded/failed states using uClaw's existing controls and checks.
- Respect reduced-motion by disabling typing/glitch/wave movement when users request reduced motion.

## Impact Targets

- GitNexus pre-change impact for `StartupSplash`: LOW, 2 direct callers (`startup-splash-preview.tsx`, `StartupSplash.test.tsx`), 0 affected execution flows.
- Expected staged detect: UI-only splash symbol and CSS/assets; no runtime/provider execution flows.

## Verification

- `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx src/components/startup/startup-splash-scenarios.test.ts`
- `cd ui && npm run build`
- `git diff --check -- <changed files>`
- `gitnexus detect_changes` on staged changes
- Browser screenshot smoke for `/startup-splash-preview.html?scenario=first-frame` and degraded/failed previews.

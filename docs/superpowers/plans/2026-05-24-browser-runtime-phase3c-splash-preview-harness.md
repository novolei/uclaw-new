# Browser Runtime Phase 3C - Startup Splash Preview Harness

## Scope

Phase 3C adds an isolated frontend preview harness for the Startup Splash and
Startup Doctor states without touching root `App` startup wiring. This gives
Phase 3 a browser-renderable screenshot target for first-frame, details,
ready, deferred, failed, reduced-motion, and core-theme checks.

- Startup Splash scenario fixtures for first-frame, details, ready, deferred,
  and failed states;
- a standalone Vite preview page at `ui/startup-splash-preview.html`;
- a preview entry point that reads `scenario`, `theme`, and `motion` query
  params;
- focused Vitest coverage for scenario resolution and state surfaces;
- tracker updates that record Phase 3B as merged and Phase 3C as current.

## ADR Section 18 Answers

1. **User intent:** startup should feel premium and trustworthy, and reviewers
   need repeatable visual evidence before the shell becomes the root route.
2. **Autonomy level:** L0 only. This slice renders deterministic preview
   states; it does not prepare, repair, download, or mutate runtime packs.
3. **Canonical truth source:** the Phase 3A/3B Startup Doctor view model remains
   the UI truth; preview scenarios are harness fixtures, not production state.
4. **TaskEvent entries:** no TaskEvents are emitted.
5. **Context read/cited:** reads ADR Phase 3 screenshot gate and tracker state.
6. **Capability cards:** no provider capability changes.
7. **Policy hooks:** no runtime action is attempted; all policy hooks remain
   backend-owned and untouched.
8. **World projection:** preview scenarios model how startup projection states
   should look once hydrated from backend status.
9. **Harness cases:** first-frame, details-expanded, ready, deferred, failed,
   reduced-motion, and core theme browser checks can target the standalone
   preview URL.
10. **Rollback/disable path:** revert the scenario helpers, preview entry/page,
    this plan, and the tracker update.
11. **Does not own:** root `App` wiring, backend IPC, runtime execution,
    downloads, cleanup, rollback, Settings UI, DB migrations, provider
    promotion, final canonical artwork, or production launch routing.

## Allowed Files

- `ui/startup-splash-preview.html`
- `ui/src/startup-splash-preview.tsx`
- `ui/src/components/startup/startup-splash-scenarios.ts`
- `ui/src/components/startup/startup-splash-scenarios.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3c-splash-preview-harness.md`

## Non-Goals

- No root `App`, `main.tsx`, Tauri command, Settings UI, DB migration, or
  browser provider changes.
- No real runtime-pack checks, network checks, downloads, archive extraction,
  deletion, Playwright launch, or provider promotion.
- No final canonical splash artwork in this slice.

## Impact Targets

- This slice adds new preview/scenario files only.
- Existing Startup Splash and Startup Doctor symbols are imported but not
  modified.
- Final staged GitNexus detect is the closeout gate.

## First Tests

- `cd ui && npm test -- --run src/components/startup/startup-splash-scenarios.test.ts src/components/startup/StartupSplash.test.tsx src/lib/startup/startup-doctor.test.ts`
- Browser preview checks against `startup-splash-preview.html` for at least
  first-frame and details-expanded states.
- `git diff --check -- <changed-files>`

The broader phase closeout should also run browser-runtime Rust regressions:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`

## Rollback

Revert the preview HTML/entry, scenario helper/tests, this plan, and the tracker
update. No persisted user data, runtime pack files, DB rows, or browser profiles
are created by this slice.

## Expected Verification Output

- Scenario and existing startup Vitest checks pass.
- Browser preview renders first-frame and details-expanded states without fresh
  startup-shell console errors.
- Rust browser runtime regressions pass with no Rust source changes.
- `rustfmt` is not required because no Rust files change.
- `git diff --check` returns no output.
- GitNexus detect-changes reports low risk and no unexpected execution flows.

# Browser Runtime Phase 3A - Startup Shell Substrate

## Scope

Phase 3 is too large for one reviewable PR because it spans branded launch
visuals, Startup Doctor state, background runtime preparation, recovery
surfaces, settings deep links, screenshots, and theme/reduced-motion gates.
This 3A slice lands the smallest reversible frontend substrate:

- a typed Startup Doctor view model for lightweight checks;
- a branded, local-first Startup Splash component with concise default status
  and expandable diagnostics;
- no root `App` integration yet, because staged GitNexus detection for that
  touch reported HIGH risk across existing top-level app processes;
- focused Vitest coverage for the state model and component behavior;
- tracker updates that record Phase 2F as merged and Phase 3A as current.

## ADR Section 18 Answers

1. **User intent:** users launching uClaw should see a fast, trustworthy startup
   state while browser runtime preparation remains visible and recoverable.
2. **Autonomy level:** L0-L1 only. This slice renders state; it does not prepare,
   repair, download, or mutate runtime packs.
3. **Canonical truth source:** the typed Startup Doctor view model. Future
   slices will hydrate it from current frontend initialization state, World
   Projection, and backend doctor reports.
4. **TaskEvent entries:** consumes planned `startup_doctor_*` and
   `browser_runtime_*` vocabulary conceptually, but emits no TaskEvents in 3A.
5. **Context read/cited:** reads local UI initialization state, static check
   descriptors, and future browser-runtime status vocabulary from the ADR and
   tracker. No external network context.
6. **Capability cards:** consumes existing Browser Runtime / Playwright runtime
   pack capability concepts; adds no provider capability cards.
7. **Policy hooks:** no runtime action is attempted, so network, destructive,
   developer fallback, identity, and task-time browser policy hooks remain
   untouched.
8. **World projection:** defines a local preview model for Startup Doctor
   projection: status line, progress, readiness/failure/degraded state, and
   optional check details.
9. **Harness cases:** component tests cover first-frame content, concise default
   diagnostics, expanded check detail, progress bounds, and failure/recovery
   status. App/root screenshot harnessing moves to Phase 3B with explicit
   GitNexus HIGH-risk review.
10. **Rollback/disable path:** revert the new startup model/component/tests,
    and this tracker/plan update.
11. **Does not own:** real browser runtime preparation, downloads, cleanup,
    rollback, IPC commands, DB migrations, Settings UI, Startup Doctor backend,
    root render error recovery, or final premium visual asset pass.

## Allowed Files

- `ui/src/components/startup/StartupSplash.tsx`
- `ui/src/components/startup/StartupSplash.test.tsx`
- `ui/src/lib/startup/startup-doctor.ts`
- `ui/src/lib/startup/startup-doctor.test.ts`
- `ui/src/assets/startup-splash/*` only if a tiny local asset is needed
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3a-startup-shell.md`

## Non-Goals

- No `tauri_commands.rs`, `main.tsx`, DB migration, runtime-pack executor, or
  browser provider changes.
- No root `App` loading-state swap in this PR; that touched top-level app
  listener/settings flows and is deferred to Phase 3B.
- No real network checks, downloads, archive extraction, deletion, or runtime
  mutation.
- No Playwright CLI/MCP launch, no provider promotion, and no browser identity
  UX.
- No final canonical splash artwork or screenshot matrix in this slice.

## Impact Targets

- New Startup Doctor model/component symbols are additive.
- The attempted `App` integration had LOW pre-change impact but HIGH staged
  detect because it participates in multiple top-level app processes; it is not
  part of the final Phase 3A diff.

## First Tests

- `cd ui && npm test -- --run src/lib/startup/startup-doctor.test.ts src/components/startup/StartupSplash.test.tsx`
- `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx`
- `git diff --check -- <changed-files>`

The broader phase closeout should also run the browser-runtime Rust regressions
requested by the tracker if local worktree runtime resources are available:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`

## Rollback

Revert the startup component/model/test files, this plan, and the tracker
update. No persisted user data, runtime pack files, DB rows, or browser
profiles are created by this slice.

## Expected Verification Output

- Startup model/component Vitest: all focused tests pass.
- Rust browser runtime regressions: existing tests pass with no source changes
  to Rust browser modules.
- `rustfmt` is not required unless a Rust file changes.
- `git diff --check` returns no output.
- GitNexus detect-changes reports low risk and no unexpected execution flows.

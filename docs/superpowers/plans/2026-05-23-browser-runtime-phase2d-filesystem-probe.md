# Browser Runtime Phase 2D - Read-Only Filesystem Probe

## Goal

Add the read-only filesystem boundary for the app-managed Browser runtime pack.
This phase turns the existing manifest/path/doctor shell into something Startup
Doctor and Settings can query later without performing downloads, extraction,
deletion, process launch, IPC, or UI work.

## Scope

- Add a manifest loader for `runtime-pack.manifest.json`.
- Add a filesystem probe that checks the expected current pack, previous pack,
  Node binary, Playwright package, worker script, and Chromium binary paths.
- Convert filesystem evidence into the existing `BrowserRuntimePackProbe` so
  the existing doctor and planner remain the policy truth.
- Export the new DTOs/functions from `browser::mod`.
- Add focused unit tests and update the phase tracker.

## Non-Goals

- No network download.
- No archive extraction.
- No filesystem deletion or cleanup executor.
- No Node/Playwright process startup.
- No Tauri commands, DB migrations, Settings UI, Startup Splash UI, or task
  runtime integration.

## ADR Section 18 Answers

1. Intent: give Browser runtime preparation a queryable local truth source.
2. Autonomy: this slice is diagnostic only; later phases decide when to act.
3. Truth source: `runtime-pack.manifest.json` plus uClaw-managed pack paths.
4. TaskEvent: no emission in this phase; later Startup Doctor maps reports to
   lightweight TaskEvents.
5. Context: reports are small serializable DTOs suitable for Settings, Doctor,
   and artifacts.
6. Capability: feeds the Browser runtime pack capability and future
   `PlaywrightCliProvider` readiness.
7. Hooks: no new hooks; policy remains in doctor/planner.
8. Projection: prepares fields needed by World Projection but does not write it.
9. Harness: unit tests cover missing, invalid, ready, and mismatch states.
10. Rollback: revert this plan, tracker update, runtime-pack DTOs/functions,
    tests, and exports.
11. Does not own: provider promotion, browser identity, app startup UX, network
    policy, or real runtime installation.

## Impact Targets

- `BrowserRuntimePackPaths`
- `BrowserRuntimePackProbe`
- `BrowserRuntimePackManifest`
- `BrowserService` export surface in `src-tauri/src/browser/mod.rs`

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

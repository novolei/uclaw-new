# Browser Runtime Phase 2F - Executor Boundary

## Goal

Add the policy-gated executor boundary for Browser runtime-pack operations so a
future filesystem/download adapter can perform install, repair, cleanup, and
rollback steps without bypassing the existing plan, doctor, artifact, and event
contracts.

## Scope

- Add a supervised execution mode and policy DTO for non-dry-run execution.
- Add a step-runner trait and outcome DTO that can be implemented by future
  filesystem/download adapters.
- Add an executor function that consumes an existing operation plan, enforces
  network/destructive policy gates, records per-step results, and stops on the
  first failed step.
- Export the new executor boundary types/functions from `browser::mod`.
- Add focused unit tests and update the Browser Runtime phase tracker.

## Allowed Files

- `src-tauri/src/browser/runtime_pack.rs`
- `src-tauri/src/browser/runtime_pack_tests.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase2f-executor-boundary.md`

## Non-Goals

- No network download.
- No archive extraction.
- No filesystem deletion, cleanup, rollback mutation, or pack promotion.
- No Node/Playwright process startup.
- No Tauri commands, DB migrations, Settings UI, Startup Splash UI, or task
  runtime integration.

## ADR Section 18 Answers

1. Intent: let runtime preparation actions advance from dry-run reports toward
   managed execution without losing policy or artifact boundaries.
2. Autonomy: diagnostic/maintenance boundary only; actual side effects remain
   blocked unless a later adapter and policy explicitly allow them.
3. Truth source: existing runtime-pack operation plan plus executor report.
4. TaskEvent: returns event names for managed execution outcomes but does not
   emit TaskEvents.
5. Context: reads manifest/path/doctor/planner output already captured in the
   operation plan; no external context.
6. Capability: consumes Browser runtime-pack capability and prepares the
   Playwright CLI runtime-pack manager.
7. Hooks: network and destructive policy gates can block execution before any
   runner step is called.
8. Projection: executor reports can later feed Startup Doctor / Settings /
   World Projection, but this slice does not write projection state.
9. Harness: unit tests cover policy block, successful runner execution, failed
   runner step, and confirmation/deferred boundaries.
10. Rollback: revert this plan, tracker update, executor DTOs/function/tests,
    and exports.
11. Does not own: real download/install/delete/rollback, provider promotion,
    identity authorization, Startup Splash UI, Settings UI, or browser task
    checkpointing.

## Impact Targets

- `execute_runtime_pack_plan_dry_run`
- `BrowserRuntimePackExecutionMode`
- `BrowserRuntimePackExecutionStatus`
- `BrowserRuntimePackStepExecutionStatus`
- `BrowserRuntimePackStepExecutionReport`
- `BrowserService` export surface in `src-tauri/src/browser/mod.rs`

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

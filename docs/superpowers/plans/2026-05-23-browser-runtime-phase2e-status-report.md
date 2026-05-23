# Browser Runtime Phase 2E - Status Report Aggregator

## Goal

Provide one read-only Browser runtime-pack status report that future Startup
Doctor and Settings surfaces can query without learning each internal module
step. The report composes the manifest/path filesystem probe, doctor outcome,
primary remediation action, operation plan, and event names.

## Scope

- Add a serializable status request/report DTO.
- Add `inspect_runtime_pack_status` to run filesystem probe, doctor, and
  operation planner in one read-only flow.
- Export the new DTOs/function from `browser::mod`.
- Add focused unit tests and update the Browser Runtime phase tracker.

## Non-Goals

- No network download.
- No archive extraction.
- No filesystem deletion or cleanup executor.
- No Node/Playwright process startup.
- No Tauri commands, DB migrations, Settings UI, Startup Splash UI, or task
  runtime integration.

## ADR Section 18 Answers

1. Intent: give runtime surfaces a single status contract before UI wiring.
2. Autonomy: diagnostic/reporting only; later phases decide user-visible action.
3. Truth source: filesystem probe plus existing doctor/planner policies.
4. TaskEvent: returns event names but does not emit TaskEvents.
5. Context: compact serializable report for Settings, Startup Doctor, and
   artifacts.
6. Capability: prepares Browser runtime readiness for Playwright CLI provider.
7. Hooks: no new hooks; existing planner retains policy gates.
8. Projection: report can be mapped into World Projection later but does not
   write projection state.
9. Harness: unit tests cover ready, offline deferred, and confirmation-required
   flows.
10. Rollback: revert this plan, tracker update, status DTOs/function, tests, and
    exports.
11. Does not own: real installation, provider promotion, identity authorization,
    Startup Splash UI, Settings UI, or browser task checkpointing.

## Impact Targets

- `diagnose_runtime_pack`
- `plan_runtime_pack_operation`
- `BrowserRuntimePackOperationRequest`
- `BrowserRuntimePackDoctorOutcome`
- `BrowserService` export surface in `src-tauri/src/browser/mod.rs`

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

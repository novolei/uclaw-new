# Phase 5B-Preflight A - Runtime Pack Runner And Readiness Probes

## Summary

This corrective slice closes the highest-risk dry-run drift found before Phase
5B: a Browser runtime pack can currently look ready from file presence alone
because worker startup and real-page probe flags default to true. Before the
Playwright CLI child worker consumes the app-managed runtime pack, readiness
must be strict and the managed executor needs a concrete local step-runner
boundary that performs policy-gated filesystem operations without relying on
global npm or user-installed Playwright.

## ADR Section 18 Questions

1. **User intent:** users should not need manual Playwright/npm setup, and
   provider readiness must not be faked before real browser-worker evidence.
2. **Autonomy level:** L1/L2 local runtime preparation boundary only. Tests use
   temp directories and local fixture files; no production network download is
   executed in this phase.
3. **Canonical truth source:** Rust-owned runtime-pack manifest, filesystem
   probe, doctor outcome, operation plan, managed execution report, and event
   names.
4. **TaskEvent entries:** no TaskEvents are emitted in this slice. It preserves
   existing `browser.runtime.*` execution event names in reports.
5. **Context read/citation:** read Browser Runtime ADR Phase 2/5, dry-run audit
   PR #453, `runtime_pack.rs`, `runtime_pack_ipc.rs`, `playwright_cli.rs`, and
   runtime-pack tests.
6. **Capability cards:** consumes `browser.playwright_cli` readiness indirectly
   by ensuring runtime reports are strict before provider selection.
7. **Policy hooks:** managed execution still blocks network/destructive steps
   unless `BrowserRuntimePackExecutorPolicy` allows them. No global npm or
   global browser cache path is introduced.
8. **World projection:** no UI change. Settings/Startup Doctor will see stricter
   status through the existing status report contract.
9. **Harness cases:** focused Rust tests prove strict readiness, local runner
   success/failure behavior, cleanup/rollback boundaries, and default browser
   runtime regressions.
10. **Rollback/disable path:** revert this PR. It changes only local code and
    tests; no runtime pack files are mutated outside tests.
11. **Does not own:** no Playwright CLI child-worker execution, no Settings
    action mutation, no live network download during verification, no provider
    promotion, no DB migration, no identity/profile UX, and no MCP lane.

## Allowed Files

- `src-tauri/src/browser/runtime_pack.rs`
- `src-tauri/src/browser/runtime_pack_runner.rs`
- `src-tauri/src/browser/runtime_pack_tests.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-runtime-pack-runner.md`

## Non-Goals

- Do not edit `agentic_loop.rs` or `tauri_commands.rs`; they are not needed for
  this runner/probe slice.
- Do not wire Settings buttons to managed execution.
- Do not spawn Node/Playwright as the Phase 5 provider child worker.
- Do not fetch from the network in tests or verification.
- Do not promote `browser.playwright_cli` or change provider default ordering.

## Impact Targets

- `BrowserRuntimePackFilesystemProbeOptions`: GitNexus impact LOW, 2 direct
  test callers, 0 affected processes.
- `probe_runtime_pack_filesystem`: GitNexus impact LOW, 4 direct callers, 15
  impacted symbols, 0 affected processes.
- `inspect_runtime_pack_status`: GitNexus impact LOW, 4 direct callers, 8
  impacted symbols, 0 affected processes.
- `execute_runtime_pack_plan_with_runner`: GitNexus impact LOW, 4 direct test
  callers, 0 affected processes.

## Implementation Steps

1. Make default filesystem probe options strict: worker startup and real page
   probe evidence default to false unless an explicit probe/runner supplies
   success.
2. Add a focused `browser::runtime_pack_runner` module with a local managed
   step runner for file-backed prepare/cleanup/rollback tests. It should fail
   closed when no app-managed archive/staging source is configured.
3. Keep runtime-pack policy gates in `execute_runtime_pack_plan_with_runner` as
   the outer enforcement boundary.
4. Add tests proving file presence alone is not ready, explicit probe success
   can be ready, local runner can install from fixture staging data, destructive
   cleanup is scoped, rollback restores the previous pack, and missing archive
   configuration fails instead of pretending success.
5. Update tracker verification and next-action rows.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/mod.rs`
- `git diff --check -- <changed-files>`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-runtime-pack-runner`

## Rollback

Revert this PR. Runtime-pack status returns to prior optimistic default probe
behavior and the local runner module disappears. No persistent user data is
changed by the PR itself.

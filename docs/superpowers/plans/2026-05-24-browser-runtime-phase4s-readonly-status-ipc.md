# Phase 4S - Readonly Browser Runtime Status IPC

## Summary

Phase 4R accepted the reviewer gate for a narrow read-only Settings IPC slice.
Phase 4S adds only a Browser Runtime status query boundary: Rust returns the
existing Phase 2 runtime-pack status report through a dedicated Tauri command,
and the frontend gets a standalone bridge wrapper. It deliberately avoids shared
`getSettings`, root `App`, Settings live wiring, runtime-pack execution, and all
provider selection.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need Browser Runtime settings and Startup Doctor surfaces to consume
   real runtime-pack status before repair/prepare controls become executable.
2. What autonomy level can it run at?
   L1 read-only local inspection. It probes paths and plans a status report but
   does not download, delete, repair, launch, or mutate runtime files.
3. What is the canonical truth source?
   Rust-owned `BrowserRuntimePackStatusReport`, derived from the pinned
   manifest, uClaw-managed runtime paths, filesystem probe, doctor, and planner.
4. What TaskEvent entries does it emit?
   None. The report includes event names for projection/preview, but the IPC
   query does not emit TaskEvents.
5. What context does it read, and how is it cited?
   It reads local uClaw-managed runtime-pack paths under `uclaw_home`, the
   default manifest, and filesystem existence. Future UI citations should use
   report fields and artifact ids once execution reports exist.
6. What capability cards does it add or consume?
   None. It consumes the existing runtime-pack contract only.
7. What policy hooks can block it?
   This read-only query is not policy-blocked. Future prepare/repair/cleanup
   commands must use runtime-pack policy gates, confirmations, and active-task
   protection.
8. What world projection does the UI render?
   No production UI wiring in this slice. Later phases can feed the report into
   Browser Runtime Settings and Startup Doctor without changing `getSettings`.
9. What harness cases prove it works?
   Focused Rust IPC-helper tests, focused frontend bridge tests, default
   browser-runtime Rust regressions, whitespace checks, and GitNexus detect.
10. What is the rollback or disable path?
   Revert this PR. It has no persistent side effects or settings migrations.
11. What does it deliberately not own?
   It does not wire Settings UI live data, execute runtime-pack actions, persist
   auto-prepare settings, edit `tauri_commands.rs`, change `getSettings`, touch
   root `App`, launch Playwright, promote providers, or mutate user data.

## Allowed Files

- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/runtime_pack_ipc.rs`
- `src-tauri/src/main.rs`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/lib/tauri-bridge.browser-runtime.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4s-readonly-status-ipc.md`

## Non-Goals

- Do not edit `tauri_commands.rs`, `agentic_loop.rs`, root `App`, DB
  migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not execute install, repair, reinstall, cleanup, rollback, downloads, or
  Playwright/browser launches.
- Do not wire Browser Runtime Settings to call the command in this PR.
- Do not change shared `getSettings` or startup initialization.

## Impact Targets

- `src-tauri/src/main.rs::main`: GitNexus impact LOW, 0 affected processes.
- `src-tauri/src/browser/runtime_pack.rs::inspect_runtime_pack_status`:
  GitNexus impact LOW, 3 direct test callers, 0 affected processes.
- `ui/src/lib/tauri-bridge.ts::getSettings`: GitNexus impact HIGH; this phase
  must not edit or call through it.
- `src-tauri/src/tauri_commands.rs::get_settings` was still unresolved by
  GitNexus; this phase avoids `tauri_commands.rs` entirely.

## Rollback

Revert this PR. The command is read-only and creates no files, settings, DB
rows, providers, browser sessions, or runtime-pack mutations.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/mod.rs src-tauri/src/main.rs`
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes`

# Browser Runtime Real State PR1 Plan

Date: 2026-05-25
Branch: `codex/browser-runtime-real-state-pr1`
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr1`

## Goal

Start the post-completion correction pass for ADR
`2026-05-23-browser-runtime-supervisor-playwright-provider.md` by making the
Rust side expose one aggregated Browser Runtime status source. This first PR
does not change app handoff, provider defaults, real runtime installation, or
browser action routing; it creates the service boundary that later PRs will use
from Splash, Settings, task-time prompts, BrowserPanel, and browser tools.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users should see and rely on real Rust-owned Browser Runtime state from
     Splash through app use, rather than UI-only projections or dry-run status.
2. What autonomy level can it run at?
   - L1/L2 local diagnostics only. This PR reads local runtime/session state and
     emits serializable status; it does not download, delete, launch a new
     provider lane, or perform browser actions.
3. What is the canonical truth source?
   - `AppState` owns a shared Browser Runtime status service backed by
     `BrowserContextManager`, runtime-pack inspection, and existing provider
     readiness contracts.
4. What TaskEvent entries does it emit?
   - None in this PR. It preserves existing event-name strings in the status
     report so later PRs can route them into canonical TaskEvents.
5. What context does it read, and how is it cited?
   - It reads runtime-pack filesystem status, active browser context session ids,
     provider readiness, and existing supervisor contract defaults. No user
     secrets, cookies, storage state, or page contents are read.
6. What capability cards does it add or consume?
   - It consumes existing `browser.local_chromium`, `browser.playwright_cli`,
     and `browser.playwright_mcp` readiness contracts. It adds no new card.
7. What policy hooks can block it?
   - GitNexus impact/detect-changes, Rust tests, and pre-commit rules for
     `uclaw_utils_home`, `memory_graph`, and SPDX headers.
8. What world projection does the UI render?
   - No UI changes. The returned status includes a Rust-side projection summary
     that later UI PRs can consume.
9. What harness cases prove it works?
   - Focused unit tests prove missing-pack, ready-pack, and active-context
     aggregation without launching browser providers or mutating runtime files.
10. What is the rollback or disable path?
   - Revert this PR. Existing `get_browser_runtime_status` fields remain
     compatible so downstream UI can keep using the old runtime-pack shape.
11. What does it deliberately not own?
   - No Splash/App handoff, Settings action execution, runtime-pack install,
     provider promotion, BrowserPanel routing, screencast deadline ownership,
     DB migration, or TaskEvent persistence.

## Implementation

- Add a focused Rust status service in `src-tauri/src/browser/` that aggregates
  runtime-pack status, active context sessions, local Chromium readiness, and
  feature-flagged Playwright readiness.
- Store the service in `AppState` beside `BrowserContextManager`, and keep the
  service thin so existing managers remain the low-level owners.
- Update `get_browser_runtime_status` to use `State<AppState>` and return the
  aggregated report while preserving all existing runtime-pack fields.
- Keep `dry_run_browser_runtime_action` unchanged except for any minimal helper
  signature adjustments needed by the IPC module.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_supervisor`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/runtime_status.rs src-tauri/src/browser/mod.rs src-tauri/src/app.rs`
- `git diff --check -- src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/runtime_status.rs src-tauri/src/browser/mod.rs src-tauri/src/app.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr1.md`
- GitNexus `detect_changes`

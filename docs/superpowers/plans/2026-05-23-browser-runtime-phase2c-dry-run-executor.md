# Browser Runtime Phase 2C Dry-Run Executor Boundary

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first runtime-pack executor boundary without performing real download, extraction, deletion, or process launch.

**Architecture:** Extend `runtime_pack.rs` with a deterministic dry-run executor that consumes `BrowserRuntimePackOperationPlan` and returns an execution report. The report records whether policy allowed execution, which steps would run, which steps are network/destructive/confirmation-gated, which event names/artifact ids should be emitted later, and whether the current pack must be retained. Later Phase 2D can swap dry-run step runners for real side-effect adapters.

**Tech Stack:** Rust, serde DTOs, existing Browser runtime-pack module, sibling unit tests, GitNexus verification.

---

## Scope

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2c-executor`
- Branch: `codex/browser-runtime-phase2c-executor`
- Base: `625193fd Merge pull request #415 from novolei/codex/browser-runtime-phase2b-install-plan`
- Source ADR: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`
- Phase: `Phase 2 - App-managed Playwright runtime pack`
- Slice: `Phase 2C - dry-run executor/report boundary`

## ADR Section 18 Answers

| Question | Phase 2C Answer |
|---|---|
| 1. What user intent does this support? | Browser runtime preparation should become executable through a visible, auditable executor path instead of jumping directly from planning to side effects. |
| 2. What autonomy level can it run at? | This slice is safe at L0-L5 because it is dry-run only. Real side effects remain disabled until a later executor adapter. |
| 3. What is the canonical truth source? | `BrowserRuntimePackOperationPlan` is the input truth; `BrowserRuntimePackExecutionReport` becomes the execution truth for Splash/Settings/doctor surfaces. |
| 4. What TaskEvent entries does it emit? | It returns intended event names and a stable artifact id, but does not emit events directly. Later wiring maps these to World Projection / TaskEvent. |
| 5. What context does it read, and how is it cited? | It reads only the operation plan. It reads no page content, no profile data, no filesystem state, and no network state beyond the plan. |
| 6. What capability cards does it add or consume? | It consumes the Phase 0 browser runtime/provider capability model and prepares the runtime-pack readiness surface for Playwright CLI. |
| 7. What policy hooks can block it? | Plan statuses `RequiresConfirmation`, `Deferred`, and `Blocked` prevent step execution. Network/destructive/confirmation flags remain visible in the report. |
| 8. What world projection does the UI render? | Splash/Settings can render operation status, blocked/deferred reason, per-step dry-run outcome, current-pack retention, and artifact id. |
| 9. What harness cases prove it works? | Focused Rust tests prove planned dry-run success, confirmation-required block, deferred block, blocked rollback, ready keep-current no-op, and destructive/network flags. |
| 10. What is the rollback or disable path? | Revert the Phase 2C DTOs/functions/tests, browser module re-exports, this plan, and tracker updates. Phase 2B planner remains intact. |
| 11. What does it deliberately not own? | No real downloader, checksum over bytes, archive extraction, filesystem deletion, Node/Playwright spawn, MCP sidecar, Tauri command, Settings UI, Splash UI, DB migration, or hosted provider. |

## Files

- Modify: `src-tauri/src/browser/runtime_pack.rs`
- Modify: `src-tauri/src/browser/runtime_pack_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Update: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Create: `docs/superpowers/plans/2026-05-23-browser-runtime-phase2c-dry-run-executor.md`

## Non-Goals

- Do not download runtime packs.
- Do not verify real archive bytes.
- Do not extract archives.
- Do not delete or move files.
- Do not spawn Node, Playwright, Chromium, or MCP.
- Do not add Tauri commands, Settings UI, Splash UI, or migrations.
- Do not modify `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`, `BrowserActionRegistry`, or `tauri_commands.rs`.

## Task 1: Add Dry-Run Executor Tests

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_tests.rs`

- [x] **Step 1: Write executor boundary tests**

Cover:

- planned prepare reports dry-run success, planned step reports, event names, artifact id, network/destructive flags, and retained current-pack intent;
- confirmation-required plans do not execute steps and report policy block;
- deferred and blocked plans do not execute steps and preserve summary/reason;
- keep-current ready plan reports no-op success;
- cleanup/rollback plans surface destructive flags in dry-run reports after user confirmation and no active tasks.

## Task 2: Add Dry-Run Executor DTOs And Logic

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack.rs`

- [x] **Step 1: Implement dry-run execution report model**

Implement:

- `BrowserRuntimePackExecutionMode`;
- `BrowserRuntimePackExecutionStatus`;
- `BrowserRuntimePackStepExecutionStatus`;
- `BrowserRuntimePackStepExecutionReport`;
- `BrowserRuntimePackExecutionReport`;
- `execute_runtime_pack_plan_dry_run(...)`.

- [x] **Step 2: Keep executor side-effect free**

The executor may derive report fields from an existing plan. It must not call `std::fs`, start processes, touch network APIs, or mutate global state.

## Task 3: Export Executor Surface

**Files:**
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Export only the new dry-run executor DTOs/functions**

Do not reorder unrelated legacy browser service code.

## Task 4: Update Tracker And Verify

- [x] **Step 1: Update close-loop tracker**

Update `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` to mark PR #415 merged and Phase 2C in progress.

- [x] **Step 2: Run focused verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
```

Expected: runtime-pack, runtime contract/supervisor, and provider tests pass.

Observed: `browser::runtime_pack` returned `22 passed; 0 failed; 2580 filtered out`;
`browser::runtime` returned `34 passed; 0 failed; 2568 filtered out`;
`browser::provider::tests` returned `6 passed; 0 failed; 2596 filtered out`.

- [x] **Step 3: Format and check whitespace**

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs
git diff --check -- src-tauri/src/browser/mod.rs src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-browser-runtime-phase2c-dry-run-executor.md
```

Expected: no output.

Observed: `rustfmt --edition 2021 --check ...` and `git diff --check -- <changed-files>`
returned no output.

- [x] **Step 4: Run GitNexus change detection**

```bash
npx gitnexus analyze
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2c-executor
```

Expected: low or medium risk limited to Browser runtime-pack module/tests/exports and docs; no affected execution flows.

Observed: GitNexus staged detect reported `risk_level: low`, `changed_files: 5`,
and `affected_processes: []`.

- [x] **Step 5: Commit and PR**

Commit message:

```bash
git commit -m "feat(browser): add runtime pack dry-run executor"
```

Commit body must list the verification commands and expected outputs. Then push and create a PR.

Observed: committed on `codex/browser-runtime-phase2c-executor` as
`feat(browser): add runtime pack dry-run executor`. PR creation follows this commit.

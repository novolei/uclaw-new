# Browser Runtime Phase 2B Install Plan Boundary

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the next app-managed Playwright runtime-pack boundary: a pure install/repair/cleanup/rollback operation planner that later Splash, Settings, and Playwright CLI provider code can call before any side effects.

**Architecture:** Extend the existing Phase 2 runtime-pack DTOs with a request/plan model for runtime-pack operations. The planner classifies whether an operation is ready, requires lightweight user confirmation, should be deferred, or is blocked, and returns ordered steps, event names, environment variables, and rollback/current-pack retention intent without downloading, extracting, deleting, or spawning anything.

**Tech Stack:** Rust, serde DTOs, existing `src-tauri/src/browser/runtime_pack.rs`, sibling Rust tests, GitNexus verification.

---

## Scope

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2b-install-plan`
- Branch: `codex/browser-runtime-phase2b-install-plan`
- Base: `49c43241 Merge pull request #414 from novolei/codex/browser-runtime-phase2-runtime-pack`
- Source ADR: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`
- Phase: `Phase 2 - App-managed Playwright runtime pack`
- Slice: `Phase 2B - install/repair/cleanup/rollback plan boundary`

## ADR Section 18 Answers

| Question | Phase 2B Answer |
|---|---|
| 1. What user intent does this support? | Users should never manually install Node, npm, Playwright, or browser binaries; the app should decide when preparation is safe, visible, confirmable, and rollbackable. |
| 2. What autonomy level can it run at? | This slice is safe at L0-L5 because it only plans operations. Later executors must enforce the returned confirmation/defer/block statuses before side effects. |
| 3. What is the canonical truth source? | The runtime-pack manifest, path policy, doctor outcome, network/trigger request, and operation plan become the canonical input for future Splash/Settings/runtime executor surfaces. |
| 4. What TaskEvent entries does it emit? | It returns intended event names such as `browser.runtime.prepare.planned`, `browser.runtime.prepare.confirmation_required`, `browser.runtime.prepare.deferred`, and rollback/cleanup variants. It does not emit events directly. |
| 5. What context does it read, and how is it cited? | It consumes explicit structs: manifest, paths, doctor outcome, operation request. It reads no page content, no user profile data, and no filesystem state. |
| 6. What capability cards does it add or consume? | It consumes the Phase 0 browser runtime/provider capability model and prepares later `browser.playwright_cli` runtime-pack readiness checks. |
| 7. What policy hooks can block it? | Offline/captive/restricted/metered network, auto-prepare disabled, active browser tasks, missing rollback, large download confirmation, and future enterprise policy can block, defer, or require confirmation. |
| 8. What world projection does the UI render? | Splash and Settings can render plan status, confirmation requirement, deferral reason, operation steps, runtime path, artifact size, rollback/current-pack retention, and next event names. |
| 9. What harness cases prove it works? | Focused Rust tests prove prepare confirmation, offline deferral, auto-prepare disabled behavior, cleanup/rollback active-task protection, missing rollback blocking, and ready/no-op plan behavior. |
| 10. What is the rollback or disable path? | Revert the new planner DTOs/functions/tests, browser module re-exports, this plan, and tracker updates. Existing Phase 2 status/doctor shell remains usable. |
| 11. What does it deliberately not own? | No network download, archive verification implementation, unpacking, filesystem deletion, process launch, Playwright worker, Tauri command, Settings UI, Splash UI, DB migration, or MCP sidecar. |

## Files

- Modify: `src-tauri/src/browser/runtime_pack.rs`
- Modify: `src-tauri/src/browser/runtime_pack_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Update: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Create: `docs/superpowers/plans/2026-05-23-browser-runtime-phase2b-install-plan.md`

## Non-Goals

- Do not download runtime packs.
- Do not verify sha256 against real bytes.
- Do not extract archives.
- Do not delete files.
- Do not spawn Node or Playwright.
- Do not add Tauri commands or Settings/Splash UI.
- Do not modify `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`, `BrowserActionRegistry`, or `tauri_commands.rs`.
- Do not add migrations or persistent settings.

## Task 1: Add Planner Tests

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_tests.rs`

- [x] **Step 1: Write tests for runtime-pack operation planning**

Cover:

- prepare plan includes download, hash verification, staging install, doctor, promotion, rollback retention, and `PLAYWRIGHT_BROWSERS_PATH`;
- metered or large downloads require lightweight user confirmation until explicitly confirmed;
- offline/captive network defers preparation without destructive or network steps;
- startup auto-prepare disabled defers only startup/background preparation;
- cleanup and rollback are deferred while active tasks are using the pack;
- rollback is blocked when no previous working pack is available;
- keep-current produces a ready no-op plan.

## Task 2: Add Planner DTOs And Logic

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack.rs`

- [x] **Step 1: Implement pure operation planner**

Implement:

- `BrowserRuntimePackOperation`;
- `BrowserRuntimePackPlanTrigger`;
- `BrowserRuntimePackNetworkState`;
- `BrowserRuntimePackPlanStatus`;
- `BrowserRuntimePackPlanStepKind`;
- `BrowserRuntimePackPlanStep`;
- `BrowserRuntimePackEnvVar`;
- `BrowserRuntimePackOperationRequest`;
- `BrowserRuntimePackOperationPlan`;
- `plan_runtime_pack_operation(...)`.

- [x] **Step 2: Keep planner side-effect free**

The planner may derive strings, paths, env vars, and event names. It must not call `std::fs`, start processes, touch network APIs, or mutate global state.

## Task 3: Export Planner Surface

**Files:**
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Export only the new planner DTOs/functions**

Do not reorder unrelated legacy browser service code.

## Task 4: Update Tracker And Verify

- [x] **Step 1: Update close-loop tracker**

Update `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` to mark PR #414 merged and Phase 2B in progress.

- [x] **Step 2: Run focused verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
```

Expected: runtime-pack, runtime contract/supervisor, and provider tests pass.

Observed: `browser::runtime_pack` returned `17 passed; 0 failed; 2580 filtered out`;
`browser::runtime` returned `29 passed; 0 failed; 2568 filtered out`;
`browser::provider::tests` returned `6 passed; 0 failed; 2591 filtered out`.

- [x] **Step 3: Format and check whitespace**

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs
git diff --check -- src-tauri/src/browser/mod.rs src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-browser-runtime-phase2b-install-plan.md
```

Expected: no output.

Observed: `rustfmt --edition 2021 --check ...` and `git diff --check -- <changed-files>`
returned no output.

- [x] **Step 4: Run GitNexus change detection**

```bash
npx gitnexus analyze
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2b-install-plan
```

Expected: low or medium risk limited to Browser runtime-pack module/tests/exports and docs; no affected execution flows.

Observed: GitNexus staged detect reported `risk_level: low`, `changed_files: 5`,
and `affected_processes: []`.

- [x] **Step 5: Commit**

Commit message:

```bash
git commit -m "feat(browser): plan runtime pack operations"
```

Commit body must list the verification commands and expected outputs.

Observed: committed on `codex/browser-runtime-phase2b-install-plan` as
`feat(browser): plan runtime pack operations`.

## Self-Review

- Spec coverage: Phase 2B should make runtime pack operations auditable before they become executable.
- Placeholder scan: no TODO/TBD placeholders in code.
- Type consistency: all public DTOs use serde rename conventions and avoid stringly typed plan statuses where enums are practical.

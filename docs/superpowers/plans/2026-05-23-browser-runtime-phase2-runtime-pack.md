# Browser Runtime Phase 2 Runtime Pack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first app-managed Playwright runtime-pack manager shell without downloading, installing, or launching Playwright.

**Architecture:** Introduce pure Rust DTOs and planning helpers for runtime-pack manifests, local path policy, doctor classifications, update policy, and remediation action planning. Later Phase 2 slices can wire actual download/install/repair/cleanup/rollback behavior behind these contracts.

**Tech Stack:** Rust, serde DTOs, existing `src-tauri/src/browser` module pattern, sibling Rust tests, GitNexus verification.

---

## Scope

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack`
- Branch: `codex/browser-runtime-phase2-runtime-pack`
- Base: `81d9b9dc Merge remote-tracking branch 'origin/codex/browser-runtime-phase1-supervisor'`
- Source ADR: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`
- Phase: `Phase 2 - App-managed Playwright runtime pack`

## ADR Section 18 Answers

| Question | Phase 2 First Slice Answer |
|---|---|
| 1. What user intent does this support? | Browser automation should work without asking users to manually install Node, npm, Playwright, or browser binaries. |
| 2. What autonomy level can it run at? | This slice is safe metadata/action planning at L0-L5; no download, install, process launch, or filesystem deletion occurs. |
| 3. What is the canonical truth source? | The runtime-pack manifest plus doctor probe shape becomes the canonical status input for later installer and Settings surfaces. |
| 4. What TaskEvent entries does it emit? | None directly in this slice. It prepares classifications that later map to `browser.startup_doctor.check`, `browser.startup_doctor.failed`, `browser.provider.degraded`, and `browser.provider.rolled_back`. |
| 5. What context does it read, and how is it cited? | It only derives paths from `uclaw_utils_home::uclaw_home_pathbuf()` or test-provided roots, and consumes explicit probe structs. It reads no browser page content. |
| 6. What capability cards does it add or consume? | It consumes the Phase 0 `browser.playwright_cli` and `browser.playwright_mcp` capability assumptions that require a runtime pack. |
| 7. What policy hooks can block it? | Future download/install actions must honor network, metered/cellular/restricted/captive/offline policy and user confirmation. This first slice only classifies and plans. |
| 8. What world projection does the UI render? | Later UI can render runtime-pack readiness, version, path, size, update state, rollback availability, and remediation labels from these DTOs. |
| 9. What harness cases prove it works? | Focused Rust tests cover manifest defaults, path derivation, doctor readiness, update policy, rollback retention, and remediation planning. |
| 10. What is the rollback or disable path? | Remove `runtime_pack.rs`, its tests, exports from `browser/mod.rs`, this plan, and tracker updates. Browser automation remains on current chromiumoxide path. |
| 11. What does it deliberately not own? | No network download, no installer, no archive extraction, no worker process, no Playwright CLI execution, no MCP sidecar, no Tauri command, no Settings UI, no DB migration. |

## Files

- Create: `src-tauri/src/browser/runtime_pack.rs`
- Create: `src-tauri/src/browser/runtime_pack_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Update: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Create/modify: `docs/superpowers/plans/2026-05-23-browser-runtime-phase2-runtime-pack.md`

## Non-Goals

- Do not modify `src-tauri/src/browser/context_manager.rs` behavior.
- Do not modify `src-tauri/src/browser/action_registry.rs`.
- Do not modify `src-tauri/src/browser/agent_loop.rs`.
- Do not modify `src-tauri/src/browser/context.rs`.
- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not add a runtime downloader, unpacker, checksum verifier implementation, or process launcher yet.
- Do not add Settings UI or startup splash wiring in this slice.
- Do not write migrations or persistent settings.

## Task 1: Add Runtime Pack Tests

**Files:**
- Create: `src-tauri/src/browser/runtime_pack_tests.rs`

- [x] **Step 1: Write tests for Phase 2 runtime-pack shell**

Cover:

- default manifest includes pack version, Node version, Playwright version, Chromium revision, worker version, minimum app version, release channel, size, sha256, URL, and rollback version;
- path policy derives a stable uClaw-managed runtime root and `PLAYWRIGHT_BROWSERS_PATH`;
- doctor classifies missing manifest, missing Node, missing Playwright package, missing browser binary, corrupt cache, version mismatch, worker startup failure, offline download, failed real-page probe, and ready;
- update policy prioritizes security updates, defers ordinary updates during active tasks, and keeps rollback available;
- remediation planner returns prepare/repair/cleanup/rollback/defer actions without touching the filesystem in tests.

## Task 2: Add Runtime Pack Module

**Files:**
- Create: `src-tauri/src/browser/runtime_pack.rs`

- [x] **Step 1: Implement pure runtime-pack data model and helpers**

Implement:

- `BrowserRuntimePackManifest`;
- `BrowserRuntimePackPaths`;
- `BrowserRuntimePackProbe`;
- `BrowserRuntimePackDoctorStatus`;
- `BrowserRuntimePackDoctorOutcome`;
- `BrowserRuntimePackIssue`;
- `BrowserRuntimePackAction`;
- `BrowserRuntimePackUpdatePolicy`;
- `BrowserRuntimePackUpdateDecision`;
- path derivation from a supplied root and from `uclaw_home_pathbuf()`;
- action planning without side effects.

- [x] **Step 2: Run focused tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
```

Expected: all `browser::runtime_pack` tests pass.

Observed: `10 passed; 0 failed; 2580 filtered out`.

## Task 3: Export Runtime Pack From Browser Module

**Files:**
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Run GitNexus impact before editing exports**

```bash
gitnexus impact BrowserService --direction upstream
```

Expected: low risk for additive browser module exports.

Observed: GitNexus impact for `Struct:src-tauri/src/browser/mod.rs:BrowserService`
reported LOW risk, 0 direct callers, and 0 affected processes.

- [x] **Step 2: Export the runtime-pack module and selected DTOs**

Only add module and re-exports; do not reorder unrelated legacy browser service code.

- [x] **Step 3: Run regression tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
```

Expected: runtime-pack, runtime contract/supervisor, and provider tests pass.

Observed: `browser::runtime_pack` returned `10 passed; 0 failed; 2580 filtered out`;
`browser::runtime` returned `22 passed; 0 failed; 2568 filtered out`;
`browser::provider::tests` returned `6 passed; 0 failed; 2584 filtered out`.

## Task 4: Verify Scope And Prepare Commit

- [x] **Step 1: Update close-loop tracker**

Update `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` with Phase 2 scope, impact, verification, and handoff notes.

- [x] **Step 2: Format and check whitespace**

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs
git diff --check -- src-tauri/src/browser/mod.rs src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-browser-runtime-phase2-runtime-pack.md
```

Expected: no output.

Observed: initial `rustfmt --check` reported formatting diffs, `rustfmt`
applied them, then `rustfmt --check` and `git diff --check` returned no output.

- [x] **Step 3: Run GitNexus change detection**

```bash
npx gitnexus analyze
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack
```

Expected: low-risk changes limited to the new runtime-pack module, browser exports, status tracker, and this plan.

Observed: after refreshing the Phase 2 worktree index, GitNexus staged detect
reported `risk_level: low`, `changed_files: 5`, and `affected_processes: []`.

- [x] **Step 4: Commit**

Commit message:

```bash
git commit -m "feat(browser): add runtime pack manager shell"
```

Commit body must list the verification commands and expected outputs.

Observed: committed on `codex/browser-runtime-phase2-runtime-pack` as current
`HEAD`: `feat(browser): add runtime pack manager shell`.

## Self-Review

- Spec coverage: Phase 2 first slice covers runtime-pack manifest, path policy, doctor status, update policy, and remediation planning.
- Placeholder scan: no TODO/TBD placeholders in code.
- Type consistency: all public DTOs use serde rename conventions and local-first path helpers.

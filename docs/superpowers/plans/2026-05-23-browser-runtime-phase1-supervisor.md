# Browser Runtime Phase 1 Supervisor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first BrowserRuntimeSupervisor layer around the current local chromiumoxide lane without changing browser action execution behavior.

**Architecture:** Introduce a pure Rust supervisor module that consumes Phase 0 contracts, owns local browser runtime session summaries, deadline policy, doctor probe classification, artifact-pack metadata, and projection snapshots. The current `BrowserContextManager` remains the concrete browser runtime; Phase 1 adds a supervised companion surface and model-free tests, while later slices wire action execution through it.

**Tech Stack:** Rust, serde DTOs, existing `src-tauri/src/browser` module pattern, sibling Rust tests, GitNexus verification.

---

## Scope

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor`
- Branch: `codex/browser-runtime-phase1-supervisor`
- Base: `84743093 feat(browser): add runtime supervisor phase0 contracts`
- Source ADR: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`
- Phase: `Phase 1 - Supervisor around the current chromiumoxide runtime`

## ADR Section 18 Answers

| Question | Phase 1 Answer |
|---|---|
| 1. What user intent does this support? | Browser tasks should expose whether the local Chromium runtime is starting, ready, acting, recovering, degraded, stopped, or waiting for user/runtime boundary instead of appearing stuck. |
| 2. What autonomy level can it run at? | Metadata and classification are safe at L0-L5. Real browser launch/action behavior remains unchanged in this slice. |
| 3. What is the canonical truth source? | Existing `BrowserContextManager` remains runtime truth for live chromiumoxide contexts. `BrowserRuntimeSupervisor` derives supervised session state/projection from explicit marks and doctor probes. |
| 4. What TaskEvent entries does it emit? | None directly in this slice. It classifies future event names from Phase 0 such as `browser.runtime.state_changed`, `browser.runtime.heartbeat_missed`, and `browser.runtime.artifact_pack_created`. |
| 5. What context does it read, and how is it cited? | It may read context-manager active sessions through existing APIs and stores timestamps/deadline metadata in memory only. No user page content is read by the pure supervisor tests. |
| 6. What capability cards does it add or consume? | It consumes the Phase 0 `browser.local_chromium` provider card and keeps Playwright lanes disabled. |
| 7. What policy hooks can block it? | None in this first supervisor shell. Later action wiring must pass SafetyManager, boundary detection, identity revocation, and ask-user hooks. |
| 8. What world projection does the UI render? | It produces `BrowserWorldProjectionSummary` values for startup doctor, runtime, identity, task boundary, degraded state, and artifact-pack references. UI wiring is deferred. |
| 9. What harness cases prove it works? | Focused Rust tests cover deadlines, legal state transitions, doctor classifications, heartbeat/action timeout degradation, artifact-pack metadata, and context-manager active session snapshots. |
| 10. What is the rollback or disable path? | Remove `runtime_supervisor.rs`, its tests, and exports from `browser/mod.rs`; no runtime behavior changes need rollback. |
| 11. What does it deliberately not own? | No Playwright runtime pack, no CLI/MCP provider, no action dispatch replacement, no Tauri command, no DB migration, no Settings UI, no startup splash. |

## Files

- Create: `src-tauri/src/browser/runtime_supervisor.rs`
- Create: `src-tauri/src/browser/runtime_supervisor_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Update: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Create/modify: `docs/superpowers/plans/2026-05-23-browser-runtime-phase1-supervisor.md`

## Non-Goals

- Do not modify `src-tauri/src/browser/context_manager.rs` behavior.
- Do not modify `src-tauri/src/browser/action_registry.rs`.
- Do not modify `src-tauri/src/browser/agent_loop.rs`.
- Do not modify `src-tauri/src/browser/context.rs`.
- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not add Playwright, MCP, hosted providers, downloads, migrations, or UI.
- Do not mark Phase 1 fully complete unless the supervisor shell, tests, tracker, and staged GitNexus detect all pass.

## Task 1: Add Supervisor Tests

**Files:**
- Create: `src-tauri/src/browser/runtime_supervisor_tests.rs`

- [x] **Step 1: Write tests for Phase 1 supervisor shell**

Cover:

- default deadline profile names startup/connect/action/wait/network idle/first frame/no-output heartbeat;
- session lifecycle starts at `starting`, can become `ready`/`idle`/`acting`, and invalid transitions degrade instead of silently succeeding;
- local chromium doctor reports `ready` when a context is active and `needs_setup`/`degraded` when classified failures occur;
- heartbeat/action timeout classification moves projection to degraded with attention reasons;
- artifact-pack metadata records reason, event name, provider id, session id, task id, and monotonic timestamp;
- context-manager active session snapshot can be converted into supervised local chromium session summaries without launching a browser in tests.

## Task 2: Add Runtime Supervisor Module

**Files:**
- Create: `src-tauri/src/browser/runtime_supervisor.rs`

- [x] **Step 1: Implement supervisor data model and helpers**

Implement:

- `BrowserRuntimeDeadlineProfile`;
- `BrowserRuntimeSupervisor`;
- `BrowserRuntimeSessionSummary`;
- `BrowserRuntimeDoctorOutcome`;
- `BrowserRuntimeDegradation`;
- `BrowserRuntimeArtifactPack`;
- state transition helper that wraps `is_allowed_browser_runtime_transition`;
- projection builder returning `BrowserWorldProjectionSummary`;
- local-chromium provider card lookup;
- context-manager active-session snapshot helper.

- [x] **Step 2: Run focused tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_supervisor
```

Expected: all `browser::runtime_supervisor` tests pass.

Observed: `7 passed; 0 failed; 2573 filtered out`.

## Task 3: Export Supervisor From Browser Module

**Files:**
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Export the supervisor module and selected DTOs**

Only add module and re-exports; do not reorder unrelated legacy browser service code.

- [x] **Step 2: Run regression tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_supervisor
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider
```

Expected: supervisor, contract, and provider tests pass.

Observed: supervisor returned `7 passed; 0 failed; 2573 filtered out`;
runtime contracts returned `5 passed; 0 failed; 2575 filtered out`; provider
returned `6 passed; 0 failed; 2574 filtered out`.

## Task 4: Verify Scope And Commit

- [x] **Step 1: Update close-loop tracker**

Update `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` with Phase 1 worktree, branch, impact, verification, and handoff notes.

- [x] **Step 2: Format and check whitespace**

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_supervisor.rs src-tauri/src/browser/runtime_supervisor_tests.rs
git diff --check -- src-tauri/src/browser/mod.rs src-tauri/src/browser/runtime_supervisor.rs src-tauri/src/browser/runtime_supervisor_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-browser-runtime-phase1-supervisor.md
```

Expected: no output.

Observed: no output for changed-file `rustfmt --check` and `git diff --check`.

- [x] **Step 3: Run GitNexus change detection**

```bash
npx gitnexus analyze
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor
```

Expected: low-risk changes limited to the new supervisor module, browser exports, status tracker, and this plan.

Observed: after refreshing the Phase 1 worktree index, GitNexus staged detect
reported `risk_level: low`, `changed_files: 5`, and `affected_processes: []`.

- [x] **Step 4: Commit**

Commit message:

```bash
git commit -m "feat(browser): add runtime supervisor phase1 shell"
```

Commit body must list the verification commands and expected outputs.

Observed: committed at current `HEAD` on `codex/browser-runtime-phase1-supervisor`
as `feat(browser): add runtime supervisor phase1 shell`.

## Self-Review

- Spec coverage: Phase 1 shell covers supervisor states, deadlines, doctor classification, artifact metadata, projection, and context-manager observation; behavior wiring remains deferred.
- Placeholder scan: no TODO/TBD placeholders in code.
- Type consistency: all public DTOs use serde rename conventions and Phase 0 contract types.

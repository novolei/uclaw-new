# Browser Runtime Phase 0 Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Phase 0 browser runtime contract skeleton from the Browser Runtime Supervisor ADR without changing browser execution behavior.

**Architecture:** Add a pure Rust contract module under `src-tauri/src/browser/` that describes supervisor states, allowed transitions, feature flags, provider capability cards, browser event names, and the initial World Projection model. Existing Local Chromium readiness metadata remains intact; this phase only creates stable DTOs and model-free tests that later supervisor/runtime phases can consume.

**Tech Stack:** Rust, serde DTOs, existing `src-tauri/src/browser` module pattern, sibling Rust tests, GitNexus verification.

---

## Scope

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts`
- Branch: `codex/browser-runtime-phase0-contracts`
- Source ADR: `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`
- Phase: `Phase 0 - Contracts, flags, and projection skeleton`

## ADR Section 18 Answers

| Question | Phase 0 Answer |
|---|---|
| 1. What user intent does this support? | Users and agents can inspect what browser runtime lanes exist, what is enabled, what is degraded, and what needs attention before runtime behavior changes land. |
| 2. What autonomy level can it run at? | Metadata only; safe at L0-L5 because it performs no browser action, process spawn, network download, profile attach, or setup mutation. |
| 3. What is the canonical truth source? | The contract module is derived/static metadata. Runtime truth remains uClaw `TaskEvent`, browser task runs, checkpoints, and future supervisor events. |
| 4. What TaskEvent entries does it emit? | None in Phase 0. It defines canonical browser event names that future phases can map to existing `TaskEvent::Warning`, `Checkpoint`, `BoundaryYield`, `ToolCall`, `ToolResult`, and `TaskFinished`. |
| 5. What context does it read, and how is it cited? | No runtime context. It encodes the accepted ADR decisions as typed constants and DTOs. |
| 6. What capability cards does it add or consume? | Adds browser provider capability cards for Local Chromium, Playwright CLI, Playwright MCP, raw CDP fallback, and hosted providers. |
| 7. What policy hooks can block it? | None for static metadata. Future setup/action/profile phases must pass SafetyManager, browser identity policy, developer-mode policy, and user-boundary hooks. |
| 8. What world projection does the UI render? | Defines projection DTOs for startup doctor, runtime state, provider lane, identity, task pause/resume, and degraded states. UI wiring is deferred. |
| 9. What harness cases prove it works? | Model-free Rust tests cover state transitions, safe default flags, capability card inventory, browser event name inventory, and projection attention derivation. |
| 10. What is the rollback or disable path? | Remove `runtime_contracts.rs`, its tests, and exports from `browser/mod.rs`; no runtime behavior changes need rollback. |
| 11. What does it deliberately not own? | No Playwright runtime pack, no CLI worker, no MCP sidecar, no browser launch, no Tauri commands, no DB migration, no Settings UI, no startup splash. |

## Files

- Create: `src-tauri/src/browser/runtime_contracts.rs`
- Create: `src-tauri/src/browser/runtime_contracts_tests.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Create/update: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- Create/modify: `docs/superpowers/plans/2026-05-23-browser-runtime-phase0-contracts.md`

## Non-Goals

- Do not modify `src-tauri/src/browser/context_manager.rs`.
- Do not modify `src-tauri/src/browser/agent_loop.rs`.
- Do not modify `src-tauri/src/browser/context.rs`.
- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not modify `src-tauri/src/db/migrations.rs`.
- Do not add Playwright, Node, MCP, hosted provider, or runtime-pack execution.
- Do not add UI wiring or settings controls in this slice.
- Do not skip the Browser Runtime close-loop tracker. Every phase must update
  `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`.

## Task 1: Add Contract Tests

**Files:**
- Create: `src-tauri/src/browser/runtime_contracts_tests.rs`

- [x] **Step 1: Write tests for Phase 0 contracts**

Covered:

- default feature flags keep risky lanes off and keep runtime auto-prepare on;
- supervisor state transition rules allow normal and recovery paths but block invalid jumps;
- provider capability cards include Local Chromium, Playwright CLI, Playwright MCP, raw CDP, and hosted provider lanes;
- Playwright CLI and MCP cards are disabled by default and tied to their feature flags;
- browser event names include startup doctor, runtime, provider, identity, pause/resume, and degraded states;
- projection summary marks degraded runtime, waiting task, revoked identity, and failed startup doctor as attention-worthy.

- [x] **Step 2: Run the focused test before implementation**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
```

Observed: before adding tests the filter returned `0 passed; 2568 filtered out`; after adding tests in the correct worktree, the implementation must make them pass.

## Task 2: Add Pure Runtime Contract Module

**Files:**
- Create: `src-tauri/src/browser/runtime_contracts.rs`

- [x] **Step 1: Implement the contract module**

Implemented:

- `BrowserRuntimeState`;
- `BrowserRuntimeTransition`;
- `is_allowed_browser_runtime_transition(from, to)`;
- `BrowserRuntimeFeatureFlags::safe_defaults()`;
- `BrowserProviderLane`;
- `BrowserProviderCapabilityCard`;
- `browser_provider_capability_cards()`;
- `browser_provider_capability_card(provider_id)`;
- `BrowserTaskEventName`;
- `browser_task_event_names()`;
- `BrowserStartupDoctorProjection`;
- `BrowserRuntimeProjection`;
- `BrowserIdentityProjection`;
- `BrowserTaskBoundaryProjection`;
- `BrowserWorldProjectionSummary`;
- `BrowserWorldProjectionSummary::attention_reasons()`.

- [x] **Step 2: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
```

Expected: all `browser::runtime_contracts` tests pass.

Observed: `5 passed; 0 failed; 2568 filtered out`.

## Task 3: Export Contracts From Browser Module

**Files:**
- Modify: `src-tauri/src/browser/mod.rs`

- [x] **Step 1: Export the contract module and selected DTOs**

Added additive module export and re-exports from `crate::browser`.

- [x] **Step 2: Run browser module tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider
```

Expected: both contract and existing provider tests pass.

Observed: contract tests returned `5 passed; 0 failed; 2568 filtered out`;
provider tests returned `6 passed; 0 failed; 2567 filtered out`.

## Task 4: Verify Scope And Commit

- [x] **Step 1: Update close-loop tracker**

Created `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` with Quick View, decision log, branch hygiene, phase loop, Phase 0 progress, and verification notes.

- [x] **Step 2: Format and check whitespace**

Run:

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs
git diff --check
```

Expected: no output.

Observed: no output for changed-file `rustfmt --check` and `git diff --check`.

- [x] **Step 3: Run GitNexus change detection**

Run:

```bash
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts
```

Expected: low-risk changes limited to the new browser runtime contract module, browser exports, status tracker, and this plan.

Observed: GitNexus staged detect on the phase worktree reported `risk_level:
low`, `changed_files: 5`, and `affected_processes: []`.

- [x] **Step 4: Commit**

Commit message:

```bash
git commit -m "feat(browser): add runtime supervisor phase0 contracts"
```

Commit body must list the verification commands and expected outputs.

Observed: committed at current `HEAD` on `codex/browser-runtime-phase0-contracts`
as `feat(browser): add runtime supervisor phase0 contracts`, with tracker and
plan closed-loop status current.

## Self-Review

- Spec coverage: covers ADR Phase 0 contracts, flags, capability cards, event names, projection skeleton, and close-loop tracker; defers runtime behavior to Phase 1+.
- Placeholder scan: no TODO/TBD placeholders.
- Type consistency: DTO and helper names are introduced before tests consume them.

# Pi Modernization Six Modules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the six architecture-review candidates from the Pi modernization report, each through its own spec, plan, review, TDD execution, verification, and code review.

**Architecture:** This is a program plan, not a single code slice. The six candidates touch independent seams, so each one gets a focused child spec and child plan before production code changes. The first child project is AgentHarness because it creates the Deep Module that later tool, plugin, session, eval, and provider work can attach to.

**Tech Stack:** Rust/Tauri backend, existing uClaw agent runtime, GitNexus, Superpowers workflow, Pi Rust reference repo, Pi TypeScript reference repo.

---

## File Structure

| File | Responsibility |
|---|---|
| `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md` | Umbrella spec for the six Modules and shared acceptance gates |
| `docs/superpowers/plans/2026-05-31-pi-modernization-six-modules.md` | Umbrella execution tracker |
| `docs/superpowers/specs/2026-05-31-agent-harness-deep-module-design.md` | Child spec for sub-project 1 |
| `docs/superpowers/plans/2026-05-31-agent-harness-deep-module.md` | Child TDD implementation plan for sub-project 1 |
| `docs/superpowers/specs/2026-06-01-tool-effects-scheduling-design.md` | Child spec for sub-project 2 |
| `docs/superpowers/plans/2026-06-01-tool-effects-scheduling.md` | Child TDD implementation plan for sub-project 2 |
| `docs/superpowers/specs/2026-06-01-plugin-runtime-design.md` | Child spec for sub-project 3 |
| `docs/superpowers/plans/2026-06-01-plugin-runtime.md` | Child TDD implementation plan for sub-project 3 |
| `docs/superpowers/specs/2026-06-01-session-store-module-design.md` | Child spec for sub-project 4 |
| `docs/superpowers/plans/2026-06-01-session-store-module.md` | Child TDD implementation plan for sub-project 4 |
| `docs/superpowers/specs/2026-06-01-evidence-gated-eval-design.md` | Child spec for sub-project 5 |
| `docs/superpowers/plans/2026-06-01-evidence-gated-eval.md` | Child TDD implementation plan for sub-project 5 |
| `docs/superpowers/specs/2026-06-01-provider-stream-module-design.md` | Child spec for sub-project 6 |
| `docs/superpowers/plans/2026-06-01-provider-stream-module.md` | Child TDD implementation plan for sub-project 6 |

## Program Gates

- [x] **Gate A: Worktree isolation**

Run:

```bash
git status --short --branch
git rev-parse --show-toplevel
```

Expected:

```text
## codex/pi-modernization-six
/Users/ryanliu/Documents/uclaw-worktrees/pi-modernization-six
```

- [x] **Gate B: Baseline audit**

Run:

```bash
rg -n "run_agentic_loop\\(" src-tauri/src/agent src-tauri/src/runtime
rg -n "ToolConcurrency|fn concurrency\\(" src-tauri/src/agent
rg -n "PluginLifecycleOwner|PluginRegistrar|mcp_servers_registered" src-tauri/src/plugins src-tauri/src/app.rs
rg -n "session_tree|SessionTree|compaction" src-tauri/src/agent src-tauri/src/db src-tauri/src/tauri_commands.rs
rg -n "Provider|Stream|StreamEvent" src-tauri/src/llm src-tauri/src/providers src-tauri/src/agent
```

Expected: output identifies the current direct callsites and seams. Save notable results into each child spec before code edits.

- [x] **Gate C: GitNexus before code**

Before editing any existing function, type, or method, run GitNexus impact on the symbol and record the blast radius in the child plan execution notes.

Expected: LOW/MEDIUM can proceed; HIGH/CRITICAL must be reported before editing.

- [x] **Gate D: TDD**

For every production-code behaviour change, write a failing test, run it, verify the expected failure, implement the minimum, and run the passing test.

Expected: each child plan has red/green command output recorded in the commit body or execution notes.

- [x] **Gate E: Review**

After each sub-project, run a code review pass against the child spec and plan.

Expected: Critical and Important findings are fixed before starting the next sub-project.

## Task 1: Child Project 1 - AgentHarness Deep Module

**Files:**
- Create: `docs/superpowers/specs/2026-05-31-agent-harness-deep-module-design.md`
- Create: `docs/superpowers/plans/2026-05-31-agent-harness-deep-module.md`
- Modify after plan review: `src-tauri/src/agent/harness.rs`
- Modify after plan review: `src-tauri/src/agent/run_assembly.rs`
- Modify after plan review: direct production callers found by `rg "run_agentic_loop\\("`
- Test after plan review: focused Rust tests under `src-tauri/src/agent/`

- [x] **Step 1: Write the child spec**

Create `docs/superpowers/specs/2026-05-31-agent-harness-deep-module-design.md` with these required sections:

```markdown
# AgentHarness Deep Module Design

**Date:** 2026-05-31
**Status:** Child spec, implementation pending
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi/packages/agent/src/harness/agent-harness.ts`, `/Users/ryanliu/Documents/pi/packages/coding-agent/src/core/agent-session.ts`, `/Users/ryanliu/Documents/pi_agent_rust/src/agent.rs`

## Problem

`AgentHarness` is currently a pass-through and direct production callers still cross the low-level `run_agentic_loop` seam.

## Goal

Make `AgentHarness` the Deep Module for run/session lifecycle execution.

## Acceptance Evidence

- Tests prove `TaskStart` and `TaskEnd` dispatch once for completed, timed-out, and cancelled runs.
- Tests prove harness-managed runs reset `ReasoningContext.force_text`.
- Tests prove cancellation token is installed before the loop starts.
- Production direct `run_agentic_loop` callers are migrated or explicitly justified as low-level internal calls.
- Focused Rust tests pass.
```

- [x] **Step 2: Write the child TDD plan**

Create `docs/superpowers/plans/2026-05-31-agent-harness-deep-module.md` with concrete red/green tasks:

1. Add failing tests for harness reset/cancellation/start-end semantics.
2. Add a harness-owned run input type if needed.
3. Move reset/cancellation installation behind `AgentHarness`.
4. Migrate `RegularTask::run` to the harness.
5. Migrate rollout/team direct production callsites or document why they stay low-level.
6. Verify with focused tests and `rg`.

- [x] **Step 3: Review the child plan**

Run:

```bash
rg -n "TODO|TBD|implement later|similar to|appropriate" docs/superpowers/plans/2026-05-31-agent-harness-deep-module.md
```

Expected: no output.

- [x] **Step 4: Execute the child plan with TDD**

Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` against the child plan. Do not edit production code before the first failing test and GitNexus impact checks.

- [x] **Step 5: Review and commit**

Run focused tests named in the child plan. Commit with a verification body.

## Task 2: Child Project 2 - Effect-Typed Tool Scheduling

**Files:**
- Create: `docs/superpowers/specs/2026-06-01-tool-effects-scheduling-design.md`
- Create: `docs/superpowers/plans/2026-06-01-tool-effects-scheduling.md`
- Modify after plan review: `src-tauri/src/agent/tools/tool.rs`
- Modify after plan review: `src-tauri/src/agent/tool_dispatch/mod.rs`
- Modify after plan review: selected builtin tool adapters

- [x] **Step 1: Write child spec from Pi Rust `ToolEffects`**
- [x] **Step 2: Write child TDD plan with red/green batch-planning tests**
- [x] **Step 3: Review child plan for placeholders and current code truth**
- [x] **Step 4: Run GitNexus impact on touched symbols**
- [x] **Step 5: Execute TDD implementation**
- [x] **Step 6: Review, verify, commit**

## Task 3: Child Project 3 - PluginRuntime Module

**Files:**
- Create: `docs/superpowers/specs/2026-06-01-plugin-runtime-design.md`
- Create: `docs/superpowers/plans/2026-06-01-plugin-runtime.md`
- Modify after plan review: `src-tauri/src/plugins/*`
- Modify after plan review: `src-tauri/src/app.rs`
- Modify after plan review: plugin example fixtures

- [x] **Step 1: Reconcile with `2026-05-31-subprocess-rpc-plugin-last-mile-design.md`**
- [x] **Step 2: Write child spec from Pi Rust extension preflight/runtime**
- [x] **Step 3: Write child TDD plan**
- [x] **Step 4: Run GitNexus impact on touched symbols**
- [x] **Step 5: Execute TDD implementation**
- [x] **Step 6: Review, verify, commit**

## Task 4: Child Project 4 - SessionStore Module

**Files:**
- Create: `docs/superpowers/specs/2026-06-01-session-store-module-design.md`
- Create: `docs/superpowers/plans/2026-06-01-session-store-module.md`
- Modify after plan review: session tree/persistence modules selected by recon

- [x] **Step 1: Write child spec from Pi Rust `SessionStoreV2` and `SessionIndex`**
- [x] **Step 2: Write child TDD plan for invariants and replay**
- [x] **Step 3: Run GitNexus impact on touched symbols**
- [x] **Step 4: Execute TDD implementation**
- [x] **Step 5: Review, verify, commit**

## Task 5: Child Project 5 - Evidence-Gated Eval Module

**Files:**
- Create: `docs/superpowers/specs/2026-06-01-evidence-gated-eval-design.md`
- Create: `docs/superpowers/plans/2026-06-01-evidence-gated-eval.md`
- Modify after plan review: `src-tauri/src/eval/*`, browser/eval adapters selected by recon

- [x] **Step 1: Write child spec from Pi validation/conformance evidence**
- [x] **Step 2: Write child TDD plan for scenario/evidence schema**
- [x] **Step 3: Run GitNexus impact on touched symbols**
- [x] **Step 4: Execute TDD implementation**
- [x] **Step 5: Review, verify, commit**

## Task 6: Child Project 6 - ProviderStream Module

**Files:**
- Create: `docs/superpowers/specs/2026-06-01-provider-stream-module-design.md`
- Create: `docs/superpowers/plans/2026-06-01-provider-stream-module.md`
- Modify after plan review: provider/LLM modules selected by recon

- [x] **Step 1: Write child spec from Pi Rust provider seam**
- [x] **Step 2: Write child TDD plan for normalized stream events**
- [x] **Step 3: Run GitNexus impact on touched symbols**
- [x] **Step 4: Execute TDD implementation**
- [x] **Step 5: Review, verify, commit**

## Final Program Task: Completion Audit

- [x] **Step 1: Re-read parent spec and all six child specs**
- [x] **Step 2: Verify every acceptance evidence item with current files and command output**
- [x] **Step 3: Run GitNexus detect-changes**
- [x] **Step 4: Run focused Rust/UI/eval commands from all child plans**
- [x] **Step 5: Run final code review**
- [x] **Step 6: Use `superpowers:finishing-a-development-branch`**

### Final Audit Evidence

Worktree isolation:

- `git status --short --branch`: `## codex/pi-modernization-six`
- `git rev-parse --show-toplevel`:
  `/Users/ryanliu/Documents/uclaw-worktrees/pi-modernization-six`

Focused verification commands:

- `cargo test --lib agent::harness -- --nocapture`: 2 passed.
- `cargo test --lib agent::run_assembly -- --nocapture`: 2 passed.
- `cargo test --lib agent::regular_task -- --nocapture`: 12 passed.
- `cargo test --lib agent::tools::tool -- --nocapture`: 21 passed.
- `cargo test --lib agent::tool_dispatch -- --nocapture`: 26 passed.
- `cargo test --lib plugins -- --nocapture`: 20 passed, 1 ignored live integration.
- `cargo test --lib agent::session_tree -- --nocapture`: 11 passed.
- `cargo test --lib eval::evidence -- --nocapture`: 7 passed.
- `cargo test --lib eval::evidence_gate -- --nocapture`: 3 passed.
- `cargo test --bin eval-evidence-gate -- --nocapture`: binary compiled, 0 tests.
- `cargo test --lib llm::provider_stream -- --nocapture`: 4 passed.
- `cargo test --lib agent::llm_stream -- --nocapture`: 6 passed.

Final code review:

- No Critical or Important findings found in the final acceptance review.
- Remaining compiler warnings are pre-existing broad-crate warnings observed
  across focused test commands and are outside this architecture slice.

GitNexus and branch finish:

- `gitnexus detect_changes(scope=staged)`: `risk_level: none`, no changed
  symbols, no affected processes.
- `superpowers:finishing-a-development-branch` was invoked after focused tests
  passed; this worktree is preserved until the integration option is chosen.

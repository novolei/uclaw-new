# Agent OS Memory Policy PR0 Contract Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the docs-only contract audit that maps current memory write/read call sites before implementation begins.

**Architecture:** This PR changes no runtime behavior. It records current gbrain, memU, memory_graph, browser artifact, hook, and TaskEvent surfaces so PR1 can add the new Memory Policy contract without guessing.

**Tech Stack:** Markdown, ripgrep, GitNexus detect-changes, existing Rust source references.

---

## File Structure

- Create: `docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`
  - Owns the PR0 evidence baseline and risk map.
- Modify: `docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`
  - Add a short PR0 audit reference after the "Current Code Truth" section.
- Modify: `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr0-contract-audit.md`
  - Mark steps as completed while executing.

## Task 1: Capture Current Memory Surfaces

**Files:**
- Create: `docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`

- [x] **Step 1: Create the audit report skeleton**

Add this file:

```markdown
# Agent OS Memory Policy Contract Audit

**Date:** 2026-05-25
**Related spec:** `docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`
**Scope:** PR0 docs-only baseline for Memory Policy Phase 1.

## Summary

This audit records the current memory/evidence surfaces that PR1-PR5 will route
through `src-tauri/src/memory_policy/`. It does not change runtime behavior.

## Current Surfaces

### gbrain writes and reads

| Surface | File | Current behavior | PR impact |
|---|---|---|---|
| Browser long-term memory | `src-tauri/src/browser/memory_adapter.rs` | Writes browser events to `MemoryStore` and schedules background `gbrain put_page`. | PR4 should replace one narrow browser evidence path after PR2 lands real gbrain/artifact targets. |
| gbrain browser UI commands | `src-tauri/src/tauri_commands.rs` | Exposes `gbrain_put_page`, `gbrain_get_page`, versions, stats through Tauri commands. | Not changed in Phase 1. |
| gbrain browse helper | `src-tauri/src/gbrain/browse.rs` | Calls `put_page` then re-fetches page detail. | PR2 can reuse this for approved durable writes. |
| chat extractor | `src-tauri/src/agent/dispatcher.rs` and `src-tauri/src/gbrain/chat_extractor.rs` | Extracts candidate facts and fires `gbrain put_page` when confidence/budget gates pass. | PR1 should not touch; future adoption can wrap the write behind Memory Policy. |

### memU recall/index surfaces

| Surface | File | Current behavior | PR impact |
|---|---|---|---|
| MemU client | `src-tauri/src/memu/client.rs` | Provides retrieval and memorize-style bridge calls over Python stdio. | PR3 can add a target adapter using existing client methods. |
| memU agent tools | `src-tauri/src/agent/tools/memu_tools.rs` | Registers memory tools and still reads memory_graph for skill usage. | PR3 should not rewrite tools; it should add an adapter first. |

### memory_graph legacy surfaces

| Surface | File | Current behavior | PR impact |
|---|---|---|---|
| legacy graph modules | `src-tauri/src/memory_graph/` | Frozen legacy/archive memory graph. | PR1 target must reject new writes with `memory_graph_frozen`. |
| recall config and engine | `src-tauri/src/memory_graph/recall.rs` | Existing legacy recall path. | PR3 may allow legacy read action only. |
| freeze policy | `BEHAVIOR.md` and `AGENTS.md` | New writes are forbidden. | All PRs preserve this rule. |

### browser evidence and artifacts

| Surface | File | Current behavior | PR impact |
|---|---|---|---|
| browser memory adapter | `src-tauri/src/browser/memory_adapter.rs` | Stores browser event payloads and schedules gbrain writes. | PR4 switches one narrow path to Memory Policy. |
| browser runtime supervisor | `src-tauri/src/browser/runtime_supervisor.rs` | Produces runtime state, artifact pack refs, and readiness evidence. | PR4 consumes event ids and artifact refs. |
| harness artifacts | `src-tauri/src/harness/artifacts.rs` and `src-tauri/src/harness/runtime.rs` | Writes JSON artifacts under harness run directories. | PR2 can use this shape for receipt artifacts. |

### hooks, policy, and events

| Surface | File | Current behavior | PR impact |
|---|---|---|---|
| HookBus | `src-tauri/src/agent/hook_bus/event.rs` and `bus.rs` | `HookEvent::MemoryWrite` is decision-capable. | PR1 executor must gate writes through this event. |
| policy evaluator | `src-tauri/src/policy_eval/spec.rs` | `ActionRequest` can represent `memory_write` actions. | PR1 can define conversion without changing evaluator behavior. |
| TaskEvent | `crates/uclaw-runtime-contracts/src/lib.rs` | Has `MemoryWrite`, `MemoryRecall`, `Signal`, and `Warning`. | Phase 1 should not add a new variant. |

## Risk Notes

- `memory_graph` writes are forbidden regardless of local policy allow.
- Browser evidence must not auto-promote to gbrain.
- gbrain and memU outages must return receipts, not silent fallbacks.
- Existing Tauri commands remain in place until later IPC shrinking work.

## Verification

- `git diff --check -- docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`
- `rg -n "memory_graph_frozen|HookEvent::MemoryWrite|TaskEvent::MemoryWrite|browser evidence" docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`
- GitNexus `detect_changes` with docs-only LOW risk.
```

- [x] **Step 2: Verify the skeleton has no markdown whitespace errors**

Run:

```bash
git diff --check -- docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md
```

Expected: no output.

- [x] **Step 3: Fill audit rows with command evidence**

Run these commands and append the high-signal matches under an "Evidence Commands" section:

```bash
rg -n "put_page|write_receipts|MemoryWrite|MemoryRecall" src-tauri/src crates/uclaw-runtime-contracts/src/lib.rs
rg -n "HookEvent::MemoryWrite|MemoryWrite \\{" src-tauri/src/agent/hook_bus src-tauri/src/policy_eval
rg -n "memory_graph.*FROZEN|memory_graph::\\{write,insert,update,delete\\}" AGENTS.md BEHAVIOR.md src-tauri/src
rg -n "artifact_ref|attach_json_artifact|HarnessArtifact" src-tauri/src/browser src-tauri/src/harness
```

Expected: each command prints file/line references or, for the freeze-regex command, at least the policy lines from `AGENTS.md` and `BEHAVIOR.md`.

## Task 2: Link The Audit From The Spec

**Files:**
- Modify: `docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`

- [x] **Step 1: Add PR0 audit reference**

Add this paragraph after "The implementation should deepen existing modules rather than introduce a new parallel memory, browser, or runtime stack.":

```markdown
The PR0 implementation audit lives at
`docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`.
It is the review baseline for PR1 and records the exact call sites that are
allowed to remain legacy until each adoption PR switches a narrow path.
```

- [x] **Step 2: Verify the spec points to the report**

Run:

```bash
rg -n "memory-policy-contract-audit" docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md
```

Expected: one match.

## Task 3: PR0 Self-Review And Commit

**Files:**
- Test: `docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`
- Test: `docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`

- [x] **Step 1: Run docs checks**

Run:

```bash
git diff --check -- docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md
rg -n 'TB[D]|TO[DO]|FIX[ME]|place' docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md
```

Expected: first command has no output; second command has no matches and exits 1.

- [x] **Step 2: Run GitNexus detect-changes**

Use GitNexus `detect_changes` with `scope=staged` after staging only these docs files.

Expected: LOW risk, no changed symbols, no affected processes.

- [x] **Step 3: Commit PR0**

Run:

```bash
git add docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr0-contract-audit.md
git commit -m "docs(agent-os): audit memory policy contract surfaces" -m "Verification: git diff --check for PR0 docs; red-flag text scan; GitNexus detect_changes scope=staged LOW with no changed symbols."
```

Expected: commit succeeds with docs-only changes.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr0-contract-audit.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch a fresh subagent for PR0, review the docs-only audit, then proceed to PR1.

**2. Inline Execution** - execute PR0 in this session with executing-plans and checkpoint before PR1.

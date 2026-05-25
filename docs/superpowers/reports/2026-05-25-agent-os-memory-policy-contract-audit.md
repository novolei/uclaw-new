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
| HookBus | `src-tauri/src/agent/hook_bus/event.rs` and `src-tauri/src/agent/hook_bus/bus.rs` | `HookEvent::MemoryWrite` is decision-capable and can be denied. | PR1 executor must gate writes through this event. |
| policy evaluator | `src-tauri/src/policy_eval/spec.rs` | `ActionRequest` can represent `memory_write` actions. | PR1 can define conversion without changing evaluator behavior. |
| TaskEvent | `crates/uclaw-runtime-contracts/src/lib.rs` | Has `MemoryWrite`, `MemoryRecall`, `Signal`, and `Warning`. | Phase 1 should not add a new variant. |

## Evidence Commands

The audit used these command families to ground the rows above.

### gbrain and memory event search

Command:

```bash
rg -n "put_page|write_receipts|MemoryWrite|MemoryRecall" src-tauri/src crates/uclaw-runtime-contracts/src/lib.rs
```

High-signal results:

- `crates/uclaw-runtime-contracts/src/lib.rs:400` defines `TaskEvent::MemoryWrite`.
- `crates/uclaw-runtime-contracts/src/lib.rs:407` defines `TaskEvent::MemoryRecall`.
- `src-tauri/src/browser/memory_adapter.rs:22` defines the browser adapter's `GBRAIN_PUT_PAGE` target.
- `src-tauri/src/browser/memory_adapter.rs:331` checks whether gbrain `put_page` is connected before browser memory writes.
- `src-tauri/src/agent/dispatcher.rs:1278` logs gbrain extractor `put_page` calls.
- `src-tauri/src/tauri_commands.rs:628` and `src-tauri/src/tauri_commands.rs:629` combine memU and gbrain write receipts in memory/gbrain harness evidence.
- `src-tauri/src/gbrain/browse.rs:373` exposes the gbrain `put_page` helper.

### Hook and policy search

Command:

```bash
rg -n "HookEvent::MemoryWrite|MemoryWrite \\{" src-tauri/src/agent/hook_bus src-tauri/src/policy_eval
```

High-signal results:

- `src-tauri/src/agent/hook_bus/event.rs:66` defines `HookEvent::MemoryWrite`.
- `src-tauri/src/agent/hook_bus/event.rs:154` maps memory writes to `HookEventKind::MemoryWrite`.
- `src-tauri/src/agent/hook_bus/bus.rs:346` tests that decision-capable memory writes can be denied.

### memory_graph freeze search

Command:

```bash
rg -n "memory_graph.*FROZEN|memory_graph::\\{write,insert,update,delete\\}|memory_graph` is FROZEN" AGENTS.md BEHAVIOR.md src-tauri/src
```

High-signal results:

- `AGENTS.md:52` states that `memory_graph` is frozen.
- `AGENTS.md:54` says new `memory_graph::{write,insert,update,delete}*` calls are blocked.
- `BEHAVIOR.md:261` repeats the frozen rule.
- `BEHAVIOR.md:263` documents the pre-commit block and runtime panic guard direction.

### browser and harness artifact search

Command:

```bash
rg -n "artifact_ref|attach_json_artifact|HarnessArtifact" src-tauri/src/browser src-tauri/src/harness
```

High-signal results:

- `src-tauri/src/harness/artifacts.rs:39` defines `HarnessArtifact`.
- `src-tauri/src/harness/runtime.rs:44` exposes `attach_json_artifact`.
- `src-tauri/src/browser/runtime_supervisor.rs:93` stores browser runtime artifact refs.
- `src-tauri/src/browser/runtime_supervisor.rs:320` formats artifact refs for runtime artifact packs.
- `src-tauri/src/browser/recipes.rs:817` requires artifact refs for browser recipe promotion candidates.

## Risk Notes

- `memory_graph` writes are forbidden regardless of local policy allow.
- Browser evidence must not auto-promote to gbrain.
- gbrain and memU outages must return receipts, not silent fallbacks.
- Existing Tauri commands remain in place until later IPC shrinking work.
- PR1 should add additive modules first and should not modify browser, gbrain,
  memU, or Tauri command call sites.

## Verification

- `git diff --check -- docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md docs/superpowers/specs/2026-05-25-agent-os-spine-phase-1-memory-policy-design.md`
- `rg -n "memory_graph_frozen|HookEvent::MemoryWrite|TaskEvent::MemoryWrite|browser evidence" docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`
- GitNexus `detect_changes` with docs-only LOW risk.

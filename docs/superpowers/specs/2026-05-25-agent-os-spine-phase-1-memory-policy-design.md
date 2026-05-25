# Agent OS Spine Phase 1 — Memory Policy Spine Design

**Date:** 2026-05-25
**Status:** Draft for review
**Source report:** `/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/architecture-review-20260525-192243.html`
**Strategic baseline:** `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`

## Purpose

The architecture review identified six deepening opportunities:

1. Make the Runtime Kernel the default path.
2. Deepen Context Fabric.
3. Turn Capability Mesh into the tool seam.
4. Collapse Browser Runtime truth.
5. Concentrate Memory Policy.
6. Shrink the Tauri IPC command port.

This design turns those findings into a staged optimization roadmap and defines
the first implementation phase in detail. The first phase is deliberately not a
full rewrite. It builds a small, testable **Memory Policy Spine** that lets
Context Fabric and Browser Runtime stop making ad hoc memory-routing decisions.

## Executive Direction

The first phase should use **Memory Policy** as the spine:

```text
Browser Runtime / Agent Loop / Automation
    -> Context Fabric
    -> Memory Policy
    -> gbrain | run ledger / harness artifacts | memU | legacy memory_graph reads
```

This preserves the Agent OS v2 rule:

- gbrain is the primary durable knowledge layer.
- run ledger, TaskEvent, and harness artifacts own evidence.
- memU is auxiliary recall, not the source of durable truth.
- memory_graph is frozen and remains legacy/read-only except for explicit
  migration allowlists.
- browser observations and checkpoints are not automatically long-term
  knowledge.

## Current Code Truth

The repo already has several useful skeletons:

- `src-tauri/src/runtime/contracts.rs` re-exports `uclaw_runtime_contracts`
  with `IntentSpec`, `TaskSpec`, and `TaskEvent`.
- `src-tauri/src/runtime/context.rs` and
  `src-tauri/src/runtime/context_tools.rs` define Context Fabric primitives.
- `src-tauri/src/agent/context_manager/manager.rs` composes prompt context,
  but much real memory and prompt assembly still happens in `tauri_commands.rs`
  and `agent/dispatcher.rs`.
- `src-tauri/src/browser/runtime_supervisor.rs` models browser runtime state,
  deadlines, doctor results, artifacts, and projection summaries.
- `src-tauri/src/browser/memory_adapter.rs` mixes browser episodic evidence,
  legacy `MemoryStore`, and gbrain writes.
- `src-tauri/src/memory_contract/adapter.rs` defines a memory adapter trait,
  but production callers still choose concrete memory paths directly.
- `src-tauri/src/tauri_commands.rs` remains a broad IPC surface with 350+
  commands and substantial domain logic.

The implementation should deepen existing modules rather than introduce a new
parallel memory, browser, or runtime stack.

## Roadmap Across All Six Findings

### 1. Concentrate Memory Policy

**Problem:** memory routing is policy-by-call-site. Browser code, memU tools,
proactive flows, gbrain extraction, and legacy UI commands each know some part
of the memory rules.

**Optimization:** introduce `src-tauri/src/memory_policy/` as the one module
that classifies knowledge and evidence. Its interface should answer:

- Is this durable knowledge?
- Is this episodic evidence?
- Is this scratch context?
- Is this a legacy read?
- Is this forbidden?
- Which adapter may act, and what receipt should be emitted?

This is the first phase because it protects the gbrain-primary direction before
Context Fabric and Browser Runtime deepen further.

### 2. Deepen Context Fabric

**Problem:** prompt assembly and context recall are split across
`tauri_commands.rs`, `agent/dispatcher.rs`, `agent/context_manager`, gbrain
prompt helpers, memory recall, and browser task memory.

**Optimization:** add a Context Fabric adapter that asks Memory Policy for
allowed recall sources and wraps the results into `ContextArtifact` values with
source, citation, and retrieval metadata. Context Fabric should not know whether
a caller is touching gbrain, memU, or legacy memory_graph.

### 3. Collapse Browser Runtime Truth

**Problem:** Browser Runtime Supervisor owns state/projection concepts, while
provider execution, browser agent loop ordering, task store, memory adapter,
and rollout bridge still carry browser runtime facts.

**Optimization:** add a browser runtime memory-policy adapter so browser
observations, checkpoints, boundaries, final states, and provider events flow
through one classification path. Evidence should produce TaskEvent/run-ledger
or harness-artifact receipts. Durable knowledge should require an explicit
policy decision and, where needed, approval/harness gating.

### 4. Turn Capability Mesh Into the Tool Seam

**Problem:** tool visibility, registration, approvals, telemetry, and event
emission are partly duplicated across chat, agent, and automation paths.

**Optimization:** after Memory Policy and Context Fabric are stable, make
Capability Mesh own tool exposure and execution receipts. The tool seam should
eventually emit policy-visible TaskEvents and harness-visible artifacts through
one module, with runtime-specific adapters.

### 5. Make Runtime Kernel the Default Path

**Problem:** `TaskSpec` and `TaskEvent` are strong vocabulary, but execution is
still partly opt-in. Direct agent-loop paths and env-gated rollout paths remain.

**Optimization:** after Memory Policy, Context Fabric, and Browser Runtime emit
consistent receipts/events, make the Runtime Kernel the default session path:
`IntentSpec -> TaskSpec -> SessionTask -> TaskEvent -> WorldProjection`.

### 6. Shrink the Tauri IPC Command Port

**Problem:** `tauri_commands.rs` is a shallow module: its interface is nearly
as broad as its implementation.

**Optimization:** use the first five changes to move domain behavior behind
deeper modules. Then split IPC into feature command adapters where commands do
request parsing, state lookup, and delegation only. This should be an outcome
of deeper modules, not the first refactor.

## Phase 1 Scope

Phase 1 covers Memory Policy plus narrow Context Fabric and Browser Runtime
integration points.

### In Scope

- Add `memory_policy` module.
- Define knowledge/evidence classification types.
- Define policy decisions and receipts.
- Add a Context Fabric adapter that wraps memory-policy recall into
  `ContextArtifact`.
- Add a Browser Runtime adapter that submits browser events to Memory Policy.
- Add focused unit tests for classification, freeze rules, receipts, and
  adapter behavior.
- Keep all old call sites working while new call sites start using the spine.

### Out of Scope

- Removing `memory_graph` commands.
- Rebuilding the memory UI.
- Replacing gbrain MCP protocol.
- Migrating all prompt assembly in one PR.
- Making Runtime Kernel the default path in Phase 1.
- Splitting `tauri_commands.rs` in Phase 1.
- Adding a schema migration unless implementation finds a minimal receipt table
  is unavoidable.

## Proposed Modules

### `src-tauri/src/memory_policy/`

Owns routing rules and receipts.

Candidate files:

- `mod.rs`
- `types.rs`
- `classifier.rs`
- `receipts.rs`
- `adapters.rs`
- `tests.rs`

Core concepts:

- `MemoryPolicyInput`
- `MemoryKnowledgeClass`
- `MemoryPolicyDecision`
- `MemoryPolicyAction`
- `MemoryReceipt`
- `MemoryPolicySource`
- `MemoryPolicyScope`

The interface should stay small:

```text
classify(input) -> MemoryPolicyDecision
apply(decision) -> MemoryReceipt
```

The first PR may implement `classify` and receipt construction without wiring
every adapter. This gives tests a stable surface before side effects move.

### `src-tauri/src/runtime/context_memory_policy.rs`

Bridges Context Fabric to Memory Policy.

Responsibilities:

- request allowed recall sources from Memory Policy;
- convert gbrain, memU, and legacy read results into `ContextArtifact`;
- preserve source/citation metadata;
- avoid direct durable writes.

This adapter should let new context code ask for memory context without
embedding gbrain/memU/memory_graph decisions at each call site.

### `src-tauri/src/browser/runtime_memory_policy.rs`

Bridges Browser Runtime to Memory Policy.

Responsibilities:

- classify browser observation/checkpoint/boundary/final-state events;
- default browser traces to episodic evidence;
- emit or return receipts for TaskEvent/run-ledger/harness paths;
- create durable gbrain write proposals only when the event is reusable
  knowledge rather than execution evidence.

The existing `BrowserLongTermMemoryAdapter` can remain as a legacy adapter
during Phase 1. The new module should make its classification rules explicit
and testable before replacing all call sites.

## Data Flows

### Browser Runtime Evidence Flow

```text
Browser observation/checkpoint/boundary/final state
  -> browser::runtime_memory_policy
  -> memory_policy::classify
  -> MemoryReceipt(evidence)
  -> TaskEvent / run ledger / harness artifact
```

Default: evidence stays evidence. A browser screenshot, checkpoint, or DOM
snapshot is not durable knowledge by itself.

### Context Fabric Recall Flow

```text
Agent turn / browser task / automation run
  -> Context Fabric
  -> runtime::context_memory_policy
  -> memory_policy::classify(read request)
  -> gbrain | memU | legacy memory_graph read
  -> ContextArtifact + citations
```

Context consumers do not choose historical memory systems directly.

### Durable Knowledge Write Flow

```text
approved fact / preference / project knowledge / correction
  -> memory_policy::classify
  -> approval or harness gate where required
  -> gbrain adapter
  -> MemoryReceipt(durable_knowledge)
  -> TaskEvent signal / diagnostics
```

Silent self-improvement writes remain forbidden.

## Error Handling

- **gbrain offline:** return a deferred or unavailable receipt. Do not fallback
  to memory_graph writes.
- **memU unavailable:** omit auxiliary recall and preserve gbrain/legacy reads.
- **memory_graph write attempt:** reject with `memory_graph_frozen`.
- **browser evidence too large:** store an artifact reference, not a gbrain page
  or prompt-sized payload.
- **ambiguous classification:** default to episodic evidence, not durable
  knowledge.
- **duplicate browser event:** receipts should support idempotency by source,
  event id, and knowledge class.
- **adapter failure after classification:** keep the decision and failed receipt
  visible for TaskEvent/diagnostics.

## Testing Strategy

Phase 1 tests should target the new interfaces.

Recommended tests:

- durable fact/preference/project note routes to gbrain action;
- browser checkpoint routes to evidence action;
- browser final state routes to evidence unless explicitly promoted;
- legacy memory_graph read is allowed;
- memory_graph durable write is rejected;
- gbrain offline returns deferred/unavailable receipt;
- memU unavailable does not fail the full context recall;
- `ContextArtifact` carries source and citation metadata;
- browser runtime adapter returns receipts without losing evidence.

Suggested verification command for the first implementation PR:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Later PRs should add focused filters for:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::context_memory_policy --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy --lib
```

## ADR Section 18 Questions

### 1. Intent

Create a policy spine that decides memory and evidence routing for Agent OS v2
subsystems.

### 2. Autonomy

Phase 1 does not raise autonomy. It makes future autonomy safer by separating
durable knowledge from evidence and by requiring explicit receipts.

### 3. Truth Source

Durable knowledge: gbrain.
Evidence: TaskEvent/run ledger/harness artifacts.
Auxiliary recall: memU.
Legacy reads: memory_graph.
Forbidden new durable writes: memory_graph.

### 4. TaskEvent

Memory decisions and receipts should be convertible into TaskEvent signals or
attached artifacts. Phase 1 may return receipts first, then wire event emission
in later PRs.

### 5. Context

Context Fabric consumes Memory Policy outputs as `ContextArtifact` values. It
does not embed gbrain/memU/memory_graph routing logic.

### 6. Capability

The Memory Policy spine prepares Capability Mesh by making memory tools and
context tools policy-visible. It does not implement full Capability Mesh in
Phase 1.

### 7. Hooks

Policy hooks can later inspect `MemoryPolicyDecision` and `MemoryReceipt`.
Phase 1 should keep the decision shape hook-friendly.

### 8. Projection

Receipts should be projection-ready: status, source, action, target, and error
fields should be explicit enough for diagnostics and World Projection.

### 9. Harness

Harness tests should validate classification, gbrain-primary routing,
memory_graph freeze behavior, and browser evidence separation.

### 10. Rollback

Rollback is low risk if Phase 1 starts with additive modules and tests.
Existing memory, browser, and context paths stay available until adapters are
switched one at a time.

### 11. What This Does Not Own

This design does not own UI redesign, gbrain protocol changes, database
migration, complete Runtime Kernel adoption, Capability Mesh execution, or IPC
module splitting.

## First Phase PR Shape

This is a design-level sequence, not the final implementation plan.

1. **PR 0 — Contract audit and test fixtures**
   - Document current memory write/read call sites.
   - Add no behavior.

2. **PR 1 — `memory_policy` classification module**
   - Add types, classifier, decisions, receipts.
   - Unit-test gbrain-primary and memory_graph-frozen rules.

3. **PR 2 — Context Fabric memory-policy adapter**
   - Add `runtime::context_memory_policy`.
   - Wrap recall results into `ContextArtifact`.

4. **PR 3 — Browser Runtime memory-policy adapter**
   - Add `browser::runtime_memory_policy`.
   - Route browser observation/checkpoint/final-state events to receipts.

5. **PR 4 — First safe call-site adoption**
   - Switch one narrow browser or context path to the new spine.
   - Keep old paths intact as fallback.

6. **PR 5 — Diagnostics and harness bridge**
   - Surface receipts in harness/diagnostic paths.
   - Prove gbrain offline and memory_graph frozen behavior.

## Acceptance Criteria

Phase 1 is complete when:

- new durable knowledge routing defaults to gbrain;
- browser evidence and durable knowledge are separated by tests;
- memory_graph durable writes are rejected by the new policy module;
- Context Fabric can retrieve memory context through Memory Policy;
- Browser Runtime can classify evidence through Memory Policy;
- no old UI or command path regresses;
- follow-up plans can target Capability Mesh, Runtime Kernel, and IPC shrinking
  without re-litigating memory ownership.

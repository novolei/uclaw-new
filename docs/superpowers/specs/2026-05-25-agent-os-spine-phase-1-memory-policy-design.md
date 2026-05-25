# Agent OS Spine Phase 1 — Memory Policy Spine Design

**Date:** 2026-05-25
**Status:** Grilled draft for implementation planning
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

The first phase should use **Memory Policy + Executor** as the spine:

```text
Browser Runtime / Agent Loop / Automation
    -> Context Fabric
    -> Memory Policy classifier
    -> explicit action list
    -> Memory Policy executor
    -> gbrain | memU | browser/harness artifacts | legacy memory_graph reads
```

This preserves the Agent OS v2 rule:

- gbrain is the primary durable knowledge layer.
- run ledger, TaskEvent, and harness artifacts own evidence.
- memU is auxiliary recall, not the source of durable truth.
- memory_graph is frozen and remains legacy/read-only except for explicit
  migration allowlists.
- browser observations and checkpoints are not automatically long-term
  knowledge.
- direct writes require policy/hook gates and receipts.
- background writes are allowed only when they first emit queued receipts.

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
  legacy `MemoryStore`, and background gbrain writes.
- `src-tauri/src/memory_contract/adapter.rs` defines a memory adapter trait,
  but production callers still choose concrete memory paths directly.
- `crates/uclaw-runtime-contracts/src/lib.rs` already has
  `TaskEvent::MemoryWrite`, `TaskEvent::MemoryRecall`, `TaskEvent::Signal`,
  and `TaskEvent::Warning`; Phase 1 does not need a new event variant.
- `src-tauri/src/agent/hook_bus/event.rs` already treats
  `HookEvent::MemoryWrite` as decision-capable.
- `src-tauri/src/harness/artifacts.rs` and `src-tauri/src/harness/runtime.rs`
  provide a JSON artifact store that can hold detailed receipt payloads.
- `src-tauri/src/tauri_commands.rs` remains a broad IPC surface with 350+
  commands and substantial domain logic.

The implementation should deepen existing modules rather than introduce a new
parallel memory, browser, or runtime stack.

The PR0 implementation audit lives at
`docs/superpowers/reports/2026-05-25-agent-os-memory-policy-contract-audit.md`.
It is the review baseline for PR1 and records the exact call sites that are
allowed to remain legacy until each adoption PR switches a narrow path.

## Roadmap Across All Six Findings

### 1. Concentrate Memory Policy

**Problem:** memory routing is policy-by-call-site. Browser code, memU tools,
proactive flows, gbrain extraction, and legacy UI commands each know some part
of the memory rules.

**Optimization:** introduce `src-tauri/src/memory_policy/` as the module that
classifies knowledge/evidence and executes explicit memory actions through
target adapters. Its interface should answer:

- Is this durable knowledge?
- Is this episodic evidence?
- Is this scratch context?
- Is this a legacy read?
- Is this forbidden?
- Which adapter may act, and what receipt should be emitted?
- Which policy/hook gates must allow the action?
- Does execution happen synchronously, with a bounded wait, or through a
  queued background completion?

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
through one classification path. Evidence should default to browser/harness
artifact receipts. Durable gbrain knowledge is allowed only after explicit
promotion, redaction review, and approval or harness gating.

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

Phase 1 covers Memory Policy, a bounded executor, and narrow Context Fabric and
Browser Runtime integration points. The executor is real, but execution is
incremental: the first implementation PR should land the contract and fake
targets before wiring every backend.

### In Scope

- Add `memory_policy` module.
- Define knowledge/evidence classification types.
- Define policy decisions, explicit action lists, execution semantics, and
  receipts.
- Add a `MemoryPolicyExecutor` that fans out only to actions named by the
  decision.
- Add target adapters for gbrain, memU, browser/harness artifacts, and
  memory_graph legacy reads.
- Reject all new memory_graph writes with `memory_graph_frozen` receipts.
- Gate direct writes through policy/hook checks before target execution.
- Add a Context Fabric adapter that wraps memory-policy recall into
  `ContextArtifact`.
- Add a Browser Runtime adapter that submits browser events to Memory Policy.
- Add focused unit tests for classification, freeze rules, receipts, and
  adapter behavior.
- Keep all old call sites working while new call sites start using the spine.

### Out of Scope

- Removing `memory_graph` commands.
- Adding any new memory_graph write path.
- Rebuilding the memory UI.
- Replacing gbrain MCP protocol.
- Migrating all prompt assembly in one PR.
- Making Runtime Kernel the default path in Phase 1.
- Splitting `tauri_commands.rs` in Phase 1.
- Adding a schema migration unless implementation finds a minimal receipt table
  is unavoidable.
- Replacing browser recipe promotion/redaction gates.

## Proposed Modules

### `src-tauri/src/memory_policy/`

Owns routing rules, execution orchestration, target receipts, and conversion to
existing runtime events.

Candidate files:

- `mod.rs`
- `types.rs`
- `classifier.rs`
- `receipts.rs`
- `executor.rs`
- `targets/mod.rs`
- `targets/gbrain.rs`
- `targets/memu.rs`
- `targets/browser_artifact.rs`
- `targets/memory_graph.rs`
- `tests.rs`

Core concepts:

- `MemoryPolicyInput`
- `MemoryKnowledgeClass`
- `MemoryPolicyDecision`
- `MemoryPolicyAction`
- `MemoryPolicyExecutionMode`
- `MemoryPolicyExecutionReceipt`
- `MemoryPolicyTargetAdapter`
- `MemoryPolicySource`
- `MemoryPolicyScope`

The interface should stay small:

```text
classify(input) -> MemoryPolicyDecision
execute(decision, deps) -> MemoryPolicyExecutionReceipt
```

`MemoryPolicyDecision` must carry an explicit action list. The executor may
fan out to multiple targets, but it must not invent additional writes. Every
target action emits its own receipt, and the executor returns an aggregate
receipt.

The first implementation PR should land the executor contract with fake targets
and the real `memory_graph` rejection target. Real gbrain, memU, and browser
artifact adapters should be wired in later PRs.

Target responsibilities:

- `targets/gbrain.rs`: execute approved durable writes through
  `gbrain::browse::put_page` or the existing gbrain MCP path.
- `targets/memu.rs`: execute auxiliary recall/index writes through
  `MemUClient`; failures degrade recall, not durable truth.
- `targets/browser_artifact.rs`: write or reference browser/harness artifacts
  for evidence payloads and receipt JSON.
- `targets/memory_graph.rs`: allow legacy reads only; all writes return
  `memory_graph_frozen`.

### `src-tauri/src/runtime/context_memory_policy.rs`

Bridges Context Fabric to Memory Policy.

Responsibilities:

- request allowed recall sources from Memory Policy;
- convert gbrain, memU, and legacy read results into `ContextArtifact`;
- preserve source/citation metadata;
- avoid direct durable writes from Context Fabric itself.

This adapter should let new context code ask for memory context without
embedding gbrain/memU/memory_graph decisions at each call site.

### `src-tauri/src/browser/runtime_memory_policy.rs`

Bridges Browser Runtime to Memory Policy.

Responsibilities:

- classify browser observation/checkpoint/boundary/final-state events;
- default browser traces to episodic evidence;
- execute browser artifact writes for evidence receipts;
- create durable gbrain actions only for promoted, redacted, reusable knowledge;
- keep source event ids and correlation ids so background completions can be
  matched back to browser events.

The existing `BrowserLongTermMemoryAdapter` can remain as a legacy adapter
during Phase 1. The new module should make its classification rules explicit
and testable before replacing all call sites.

## Data Flows

### Target Matrix

| Input class | Default actions | Notes |
|---|---|---|
| Durable user/project/domain fact | `gbrain_write` | gbrain is canonical durable truth. |
| Auxiliary recall/index hint | `memu_write_or_index` | memU is non-canonical and may degrade. |
| Browser observation/checkpoint/final state | `browser_artifact_write` | Browser evidence does not auto-promote. |
| Promoted browser knowledge | `browser_artifact_write`, `gbrain_write` | Requires redaction plus approval or harness gate. |
| memory_graph read | `memory_graph_read` | Legacy reads only. |
| memory_graph write | rejected receipt | Always `memory_graph_frozen`. |

### Browser Runtime Evidence Flow

```text
Browser observation/checkpoint/boundary/final state
  -> browser::runtime_memory_policy
  -> memory_policy::classify
  -> MemoryPolicyDecision(actions = [browser_artifact_write])
  -> MemoryPolicyExecutor
  -> browser/harness artifact target
  -> MemoryPolicyExecutionReceipt(evidence)
  -> TaskEvent::MemoryWrite or TaskEvent::Signal
```

Default: evidence stays evidence. A browser screenshot, checkpoint, or DOM
snapshot is not durable knowledge by itself.

### Context Fabric Recall Flow

```text
Agent turn / browser task / automation run
  -> Context Fabric
  -> runtime::context_memory_policy
  -> memory_policy::classify(read request)
  -> MemoryPolicyDecision(actions = allowed recalls)
  -> gbrain | memU | legacy memory_graph read targets
  -> ContextArtifact + citations + MemoryPolicyExecutionReceipt
```

Context consumers do not choose historical memory systems directly.

### Durable Knowledge Write Flow

```text
approved fact / preference / project knowledge / correction
  -> memory_policy::classify
  -> policy/hook gate
  -> gbrain target adapter
  -> MemoryPolicyExecutionReceipt(durable_knowledge)
  -> TaskEvent::MemoryWrite + diagnostics
```

Silent self-improvement writes remain forbidden.

### Promotion Gate

Browser evidence may become durable gbrain knowledge only when it is stable,
reusable, and redacted. Allowed promotion content includes stable URL patterns,
selector notes, wait conditions, auth boundary descriptions, reusable
troubleshooting conclusions, and user-approved durable facts.

The gate rejects or defers:

- screenshots, raw DOM snapshots, OCR payloads, and raw page dumps;
- cookies, storage state, tokens, secret values, or secret handles;
- private user data such as emails, orders, account details, or payment data;
- transient pixel coordinates and one-off page state;
- task diaries and step-by-step browser run transcripts;
- private API shapes without auth-boundary review;
- any browser-derived durable write without `redaction_status=clean_reviewed`
  or an `approval_ref` / `harness_case_ids` trail.

Rejected promotion should still preserve evidence through
`browser_artifact_write` and emit a `promotion_rejected_or_deferred` receipt.

### Execution Semantics

| Action | Default execution | Receipt status |
|---|---|---|
| `browser_artifact_write` | synchronous or short bounded wait | `succeeded` / `failed` |
| `gbrain_write` for explicit durable facts | bounded await | `succeeded` / `deferred` / `failed` |
| `gbrain_write` for promoted browser knowledge | bounded await or queued | `queued` then completion receipt |
| `memu_write_or_index` | optional bounded await or queued | `succeeded` / `unavailable` / `degraded` |
| `memory_graph_write` | never executes | `rejected(memory_graph_frozen)` |

No path may run as untracked background work. If an action continues in the
background, the executor first emits a queued receipt with `action_id`,
`source_event_id`, and `correlation_id`. Later completion emits a second
receipt or TaskEvent-compatible signal.

### Policy and Hook Gate

Every write action must pass policy and hook checks before target execution:

```text
MemoryPolicyAction(write)
  -> ActionRequest(action_class = "memory_write", target, risk_class)
  -> HookEvent::MemoryWrite(task_id, topic, size_bytes)
  -> allow | deny | ask
```

- `deny` returns `rejected(policy_denied)` without target execution.
- `ask` returns `deferred(approval_required)` and must not continue in the
  background.
- `allow` permits target execution.
- `memory_graph_write` remains rejected even if policy/hook gates allow it;
  the freeze rule has higher priority than local policy.

### Receipt Contract

`MemoryPolicyExecutionReceipt` is the minimum auditable contract:

```text
receipt_id
decision_id
action_id
source
source_event_id
task_id
intent_id?
correlation_id
knowledge_class
action
target
status
reason_code?
artifact_ref?
target_ref?
idempotency_key
created_at
completed_at?
error?
```

`status` values are:

```text
planned | allowed | queued | succeeded | deferred | degraded | rejected | failed
```

`reason_code` must be enum-like and testable. Initial values should include
`memory_graph_frozen`, `policy_denied`, `approval_required`,
`gbrain_unavailable`, `queued_for_background_write`, `redaction_required`,
`promotion_rejected_or_deferred`, and `target_error`.

Receipts should map to existing runtime events:

- successful writes: `TaskEvent::MemoryWrite { target, artifact_ref }`;
- recalls: `TaskEvent::MemoryRecall { target, artifact_ref }`;
- queued/deferred/rejected/degraded outcomes: `TaskEvent::Signal` or
  `TaskEvent::Warning` plus receipt artifact.

Phase 1 should not add a new `TaskEvent` variant.

## Error Handling

- **gbrain offline:** return a deferred or unavailable receipt. Do not fallback
  to memory_graph writes.
- **memU unavailable:** return `degraded` or `unavailable` receipts and preserve
  gbrain, browser artifact, and legacy read paths.
- **memory_graph write attempt:** reject with `memory_graph_frozen`.
- **browser evidence too large:** store an artifact reference, not a gbrain page
  or prompt-sized payload.
- **ambiguous classification:** default to episodic evidence, not durable
  knowledge.
- **duplicate browser event:** receipts should support idempotency by source,
  event id, target, and knowledge class.
- **adapter failure after classification:** keep the decision and failed receipt
  visible for TaskEvent/diagnostics.
- **policy denial:** return a rejected receipt and do not invoke the target
  adapter.
- **approval required:** return a deferred receipt and do not queue hidden work.
- **background completion failure:** emit a completion receipt or warning tied
  to the original `correlation_id`.

## Testing Strategy

Phase 1 tests should target the new interfaces.

Recommended tests:

- durable fact/preference/project note routes to gbrain action;
- direct write actions call policy/hook gates before target execution;
- browser checkpoint routes to evidence action;
- browser final state routes to evidence unless explicitly promoted;
- promoted browser knowledge requires redaction/approval/harness metadata;
- legacy memory_graph read is allowed;
- memory_graph durable write is rejected;
- memory_graph write remains rejected even after policy allow;
- gbrain offline returns deferred/unavailable receipt;
- memU unavailable does not fail the full context recall;
- queued background work produces a queued receipt with correlation metadata;
- receipt fields include action/source/task/correlation/idempotency fields;
- receipts convert to existing `TaskEvent::MemoryWrite`, `MemoryRecall`,
  `Signal`, or `Warning` without adding a new runtime contract variant;
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

Create a policy and executor spine that classifies memory/evidence, executes
approved writes, and emits auditable receipts for Agent OS v2 subsystems.

### 2. Autonomy

Phase 1 does not raise autonomy. It makes future autonomy safer by separating
durable knowledge from evidence, requiring policy/hook gates for writes, and
requiring explicit receipts for synchronous and background work.

### 3. Truth Source

Durable knowledge: gbrain.
Evidence: TaskEvent/run ledger/harness artifacts.
Auxiliary recall: memU.
Legacy reads: memory_graph.
Forbidden new durable writes: memory_graph.

### 4. TaskEvent

Receipts map to existing `TaskEvent::MemoryWrite`, `TaskEvent::MemoryRecall`,
`TaskEvent::Signal`, and `TaskEvent::Warning`. Phase 1 does not add a new
runtime contract variant.

### 5. Context

Context Fabric consumes Memory Policy outputs as `ContextArtifact` values. It
does not embed gbrain/memU/memory_graph routing logic.

### 6. Capability

The Memory Policy spine prepares Capability Mesh by making memory tools and
context tools policy-visible. It does not implement full Capability Mesh in
Phase 1.

### 7. Hooks

All write actions must pass `ActionRequest(action_class = "memory_write")` and
`HookEvent::MemoryWrite` before target execution. A deny rejects the action, an
ask defers it, and `memory_graph_frozen` still wins over an allow.

### 8. Projection

Receipts should be projection-ready: status, source, source event, action,
target, artifact ref, target ref, reason code, and error fields should be
explicit enough for diagnostics and World Projection.

### 9. Harness

Harness tests should validate classification, gbrain-primary routing,
memory_graph freeze behavior, browser evidence separation, receipt emission,
promotion gates, and queued completion correlation.

### 10. Rollback

Rollback is low risk if Phase 1 starts with additive modules and tests.
Existing memory, browser, and context paths stay available until adapters are
switched one at a time.

### 11. What This Does Not Own

This design does not own UI redesign, gbrain protocol changes, database
migration, complete Runtime Kernel adoption, Capability Mesh execution, or IPC
module splitting. It also does not own new memory_graph writes, replacement of
browser recipe promotion gates, or a new TaskEvent variant.

## First Phase PR Shape

This is a design-level sequence, not the final implementation plan.

1. **PR 0 — Contract audit and test fixtures**
   - Document current memory write/read call sites.
   - Identify existing gbrain, memU, browser artifact, hook, and TaskEvent
     surfaces.
   - Add no behavior.

2. **PR 1 — Executor contract and fake targets**
   - Add `memory_policy` types, classifier, decisions, action lists, receipts,
     executor, and target adapter trait.
   - Add fake gbrain/memU/browser artifact targets.
   - Add the real memory_graph rejection target.
   - Unit-test gbrain-primary, receipt shape, TaskEvent mapping, hook-gate
     behavior, and memory_graph-frozen rules.

3. **PR 2 — gbrain and artifact targets**
   - Wire approved durable writes to the existing gbrain path.
   - Wire evidence receipts to browser/harness artifact references.
   - Implement bounded await, queued receipts, and completion correlation.

4. **PR 3 — memU target and Context Fabric adapter**
   - Wire auxiliary memU writes/indexing and unavailable/degraded receipts.
   - Add `runtime::context_memory_policy`.
   - Wrap allowed recall results into `ContextArtifact`.

5. **PR 4 — Browser Runtime adoption**
   - Add `browser::runtime_memory_policy`.
   - Switch one narrow path from `BrowserLongTermMemoryAdapter` or adjacent
     browser evidence handling to the new spine.
   - Keep old paths intact as fallback.
   - Prove browser evidence defaults to artifact and promoted browser knowledge
     requires redaction/approval/harness metadata.

6. **PR 5 — Diagnostics and harness bridge**
   - Surface receipts in harness/diagnostic paths.
   - Prove gbrain offline, memU unavailable, queued completion, and
     memory_graph frozen behavior.

## Acceptance Criteria

Phase 1 is complete when:

- new durable knowledge routing defaults to gbrain;
- direct writes pass policy/hook gates before target execution;
- executor fan-out is explicit in `MemoryPolicyDecision.actions`;
- every target action emits a receipt with correlation and idempotency fields;
- background work is never receipt-less;
- browser evidence and durable knowledge are separated by tests;
- browser-derived gbrain writes require promotion metadata;
- memory_graph durable writes are rejected by the new policy module;
- Context Fabric can retrieve memory context through Memory Policy;
- Browser Runtime can classify evidence through Memory Policy;
- receipts map to existing TaskEvent variants without changing runtime
  contracts;
- no old UI or command path regresses;
- follow-up plans can target Capability Mesh, Runtime Kernel, and IPC shrinking
  without re-litigating memory ownership.

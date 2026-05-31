# Pi Modernization Six Modules Design

**Date:** 2026-05-31
**Status:** Program spec, implemented and audit complete
**Reference report:** `/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/architecture-review-20260531T114655Z.html`
**Reference repos:** `/Users/ryanliu/Documents/pi_agent_rust`, `/Users/ryanliu/Documents/pi`
**Strategic baseline:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`

## Problem

uClaw has already absorbed several Pi ideas: `AgentApi`, dual queues,
`TurnSnapshot`, `NextTurnPatch`, `MemoryAdapter`, and `RollingTailBuffer`.
The remaining architecture gap is not a missing feature list. It is missing
depth at six seams where Pi has modern Modules with small Interfaces and
larger Implementations.

The six report candidates are therefore treated as six implementation
sub-projects. Each sub-project must get its own spec, plan, plan review, TDD
execution, verification, and final code review before it is considered done.

## Goals

1. Deepen the `AgentHarness` seam so session/run lifecycle knowledge is not
   spread across direct `run_agentic_loop` callers.
2. Replace the shallow `ToolConcurrency` bit with effect-typed tool scheduling
   influenced by Pi Rust `ToolEffects`.
3. Turn plugin discovery/registration into a `PluginRuntime` Module that owns
   preflight, trust, MCP config contribution, status, and shutdown/kill
   semantics.
4. Consolidate session tree, persistence, replay, and fast index behaviour into
   a `SessionStore` Module that keeps uClaw's SQLite-first posture while
   borrowing Pi Rust `SessionStoreV2` invariants.
5. Add an evidence-gated `Eval` Module so browser, agent, plugin, provider, and
   session claims pass through one evidence schema and replay seam.
6. Narrow the provider streaming seam into a `ProviderStream` Module with
   normalized stream events and adapter-specific provider quirks hidden behind
   one Interface.

## Non-Goals

- Do not replace uClaw's Tauri desktop product shape with Pi's CLI shape.
- Do not move durable application data out of SQLite merely because Pi uses
  JSONL or segmented stores. Borrow invariants, not storage fashion.
- Do not create a new plugin transport if existing MCP subprocess transport can
  satisfy the seam.
- Do not expand `agentic_loop.rs` or `tauri_commands.rs` as the default place
  for new behaviour. They should stay orchestration/IPC shims.
- Do not write to `memory_graph`; it remains frozen.

## Current Code Truth

- `src-tauri/src/agent/harness.rs` is a pass-through into
  `run_assembly::run_agent`.
- `src-tauri/src/agent/run_assembly.rs` owns timeout/cancellation and
  `TaskStart`/`TaskEnd` hook dispatch, but production callers still call
  `run_agentic_loop` directly in `regular_task.rs`, `rollout_integration.rs`,
  and `teams/worker.rs`.
- `src-tauri/src/agent/tools/tool.rs` exposes `ToolConcurrency` plus several
  separate risk/safety/preview methods.
- `src-tauri/src/plugins/lifecycle.rs` and `registration.rs` discover and
  register plugin contributions, but the live primary checkout's plugin
  lifecycle is still closer to accounting than runtime ownership.
- `src-tauri/src/agent/session_tree.rs`, session persistence, compaction, and
  Tauri commands know related but separate session invariants.
- Provider request assembly, stream parsing, model routing, and provider
  options are split across `llm`, `providers`, and agent dispatcher modules.

## Pi Reference Truth

- Pi TypeScript `packages/agent/src/harness/agent-harness.ts` owns session,
  phase, abort, queued messages, resources, stream options, hooks, compaction,
  and event subscribers.
- Pi TypeScript `packages/coding-agent/src/core/agent-session.ts` turns CLI mode
  diversity into one session lifecycle Module.
- Pi Rust `src/tools.rs` exposes `ToolEffects`; Pi Rust `src/agent.rs` plans
  compatible tool batches and records batch-plan evidence.
- Pi Rust `src/session_store_v2.rs` and `src/session_index.rs` separate durable
  write invariants from fast list/resume queries.
- Pi Rust `src/extensions.rs` and `src/extension_preflight.rs` treat extension
  loading as a runtime with preflight, trust, policy, host calls, and kill
  semantics.
- Pi Rust `src/provider.rs` keeps provider streaming behind a small trait using
  borrowed `Context<'a>` and normalized stream options.

## Sub-Project 1: AgentHarness Deep Module

**Objective:** Make `AgentHarness` the Interface for run/session lifecycle
execution while leaving `agentic_loop` as Implementation.

**Borrow from Pi:** Phase ownership, queued messages, stream/run options,
normalized harness errors, event subscribers, and compaction hook placement.

**uClaw adaptation:** Start with the existing Rust `run_assembly` path and
move direct `run_agentic_loop` callers behind a harness entrypoint. Keep the
first slice small: a typed `AgentHarnessRun` / `AgentHarness` Module that
centralizes force-text reset, cancellation token installation, timeout,
`TaskStart`/`TaskEnd`, and rollout/event emission hooks.

**Acceptance evidence:**

- Tests prove the harness emits start/end once for completed, timed-out, and
  cancelled runs.
- Tests prove callsites no longer need to set `force_text = false` themselves
  for harness-managed runs.
- Searches show production direct `run_agentic_loop` calls are either removed
  or explicitly documented as low-level internal/test-only calls.
- Focused Rust tests pass for `agent::harness`, `agent::run_assembly`,
  `agent::regular_task`, and touched rollout/team modules.

## Sub-Project 2: Effect-Typed Tool Scheduling

**Objective:** Replace shallow concurrency selection with a deeper tool effects
Interface.

**Borrow from Pi:** `ToolEffects::{read, write, append, network, process}`,
`parallel_safe`, `compatible_with`, batch-plan evidence, fail-closed defaults.

**uClaw adaptation:** Keep existing `Tool` trait stable enough for migration by
adding `effects()` first, deriving `concurrency()` from effects during the
transition, then moving dispatcher batching to effect compatibility.

**Acceptance evidence:**

- Tests prove read/read tools batch together, write/process tools create
  barriers, unknown tools fail closed as write/process or sequential.
- Dispatcher tests assert batch-plan evidence and cancellation behaviour.
- Existing approval/path preview tests still pass.

## Sub-Project 3: PluginRuntime Module

**Objective:** Turn plugin lifecycle from manifest accounting into runtime
ownership.

**Borrow from Pi:** Extension preflight report, trust/policy gates, runtime
status, kill switch, and host-call style isolation vocabulary.

**uClaw adaptation:** Reuse existing MCP as subprocess/RPC transport. Complete
the current plugin last-mile work, then add preflight/trust/status/reload/kill
without adding a new transport.

**Acceptance evidence:**

- Plugin manifest with subprocess permission contributes MCP server configs.
- Plugin preflight returns machine-readable findings and blocks unsafe launch
  before `McpManager` spawn.
- Runtime status exposes loaded/skipped/failed/killed plugins.
- Tests cover discovery, registrar, lifecycle aggregation, preflight, and
  runtime kill semantics.

## Sub-Project 4: SessionStore Module

**Objective:** Consolidate session tree, persistence, compaction anchors, and
fast index behaviour behind one Interface.

**Borrow from Pi:** Atomic write invariants, segmented/manifest integrity,
parent-entry tree invariants, sidecar index refresh, stale-index detection.

**uClaw adaptation:** Preserve SQLite as the primary store unless a focused ADR
reopens that decision. Model Pi's invariants as a Rust Module that can use
SQLite transactions and an optional export/replay adapter.

**Acceptance evidence:**

- Tests prove parent/branch invariants, replay order, stale index refresh, and
  compaction anchor lookup.
- Existing session-tree UI and Tauri commands call the new Module instead of
  re-implementing invariants.

## Sub-Project 5: Evidence-Gated Eval Module

**Objective:** Make verification claims pass through one evidence schema and
replay seam.

**Borrow from Pi:** Conformance suites, VCR/replay thinking, validation broker,
machine-readable evidence records, no-data fail-closed gates.

**uClaw adaptation:** Start with local eval/browser/plugin/provider/session
scenarios already in repo, then normalize their outputs into a single evidence
Module.

**Acceptance evidence:**

- Scenario manifest parser and evidence schema have unit tests.
- Existing browser/agent/plugin checks can emit evidence records.
- A CI/local command fails closed when a required evidence record is missing.

## Sub-Project 6: ProviderStream Module

**Objective:** Hide provider quirks and stream parsing behind one ProviderStream
Interface.

**Borrow from Pi:** `Provider` trait shape, borrowed request context, normalized
stream events, stream options, provider-specific adapters.

**uClaw adaptation:** Keep existing provider implementations, but add a narrow
Module that accepts assembled context and emits normalized events. Migrate one
provider adapter first, then expand.

**Acceptance evidence:**

- Tests prove normalized event ordering for text, tool calls, reasoning, and
  provider errors.
- One provider path uses the ProviderStream Module end-to-end.
- Existing model swap/BYOK/provider hardening tests remain green.

## Execution Order

1. AgentHarness Deep Module.
2. Effect-Typed Tool Scheduling.
3. PluginRuntime Module.
4. SessionStore Module.
5. Evidence-Gated Eval Module.
6. ProviderStream Module.

This order maximizes leverage: the harness seam becomes the place later tool,
plugin, session, eval, and provider Modules can plug into without spreading
new facts across callers.

## Required Process Per Sub-Project

For each sub-project:

1. Write or update a focused spec in `docs/superpowers/specs/`.
2. Write a focused implementation plan in `docs/superpowers/plans/`.
3. Self-review the plan against the spec and current code truth.
4. Run GitNexus impact before editing existing code symbols.
5. Use TDD: failing test first, verify red, implement, verify green.
6. Commit narrow slices with verification commands in commit bodies.
7. Request code review at the end of the sub-project.
8. Fix critical/important review findings before moving to the next
   sub-project.

---
title: Symphony Runtime — Visual DAG Execution Workspace
date: 2026-05-17
status: draft
authors: [@ryan]
related:
  - docs/superpowers/specs/2026-05-17-symphony-runtime-design.md (this file)
  - docs/superpowers/plans/symphony-runtime.md (commit-by-commit plan)
  - https://github.com/openai/symphony/blob/main/SPEC.md (upstream)
  - src-tauri/src/automation/runtime/service.rs (sister runtime)
  - src-tauri/src/agent/headless.rs (reused executor)
---

# Symphony Runtime — Visual DAG Execution Workspace

A third top-level execution runtime for uClaw, parallel to **Chat** and **Agent**, that turns OpenAI's [Symphony spec](https://github.com/openai/symphony/blob/main/SPEC.md) into a desktop-native, visually-orchestrated, multi-step task pipeline. Symphony in uClaw is **not a daemon polling a Linear board** (that is one optional source); it is a **DAG-of-agent-runs** workspace where each node is one `HeadlessDelegate` invocation, edges describe handoffs, and the whole graph is observable, recoverable, and cost-bounded by the same machinery that already powers the automation runtime.

Three sentences of why this is worth building inside uClaw rather than as a standalone Elixir port. (1) uClaw already owns every Symphony moving part — `run_agentic_loop`, `HeadlessDelegate`, `ToolRegistry`, `SafetyManager`, `CostCapState`, `ServiceManager`, `InfraService`, `agent_sessions` — they are not duplicated, only re-arranged. (2) Symphony's central insight (per-task isolated workspace + WORKFLOW.md as code + state-machine-with-supervisor) maps cleanly onto uClaw's existing per-spec workspace + `automation_specs` + `AppRuntimeService` design, so the adaptation is mechanical rather than re-architectural. (3) A visual canvas is the natural desktop counterpart to Linear-as-control-plane: users author and observe orchestration in one place instead of round-tripping through an external tracker.

## 1. Goals and non-goals

### 1.1 In scope

- A new `SymphonyService` that satisfies `ManagedService`, registered alongside `AppRuntimeService` in `main.rs` Stage 3, gated by `MemubotConfig.symphony.enabled`.
- A new `symphony/` Rust module under `src-tauri/src/` mirroring the layout of `automation/`: `runtime/`, `protocol/`, `sources/`, `tools/` (optional), `manager.rs`.
- A new **SymphonyCanvas** React view registered as `tab.type === 'symphony'`, with `@xyflow/react` as the node-graph engine, theme-token-driven styling, IPC-event-driven live updates.
- A new `AppMode = 'chat' | 'agent' | 'symphony'` segment in `ModeSwitcher`, with session restoration symmetric to the existing two modes.
- One migration (**V33** — see §8.1, this corrects the stale registry in `CLAUDE.md` which still lists V26 as in progress) introducing `symphony_workflows`, `symphony_workflow_versions`, `symphony_runs`, `symphony_nodes`, `symphony_node_runs`, `symphony_edges`, and a `symphonies` row in `spaces`.
- Reuse of `HeadlessDelegate` (renamed conceptually to "step executor", but the type is unchanged) so every Symphony node is one full agentic-loop run with the existing tool set, the existing safety machinery, the existing cost accounting, the existing context compression.
- Cost guardrails per-node, per-run, per-day, identical in shape to `AutomationConfig.{per_run_cost_cap_usd, per_day_cost_cap_usd}` and persisted through `cost_records` (V13).
- Recovery: every node's state lives in SQLite; on restart `SymphonyService::start()` reconciles in-memory state from `symphony_node_runs` and re-spawns `Running` nodes (or marks them `Stalled` if they crossed `stall_timeout_ms` while the app was down).
- Observability: every node lifecycle transition emits an `InfraEvent` on the existing bus + a `symphony:node_update` Tauri event for the canvas.

### 1.2 Out of scope (deferred)

- Linear/Jira/GitHub-issue ingestion as a workflow **source**. The trait is defined; the default desktop usage is **manual canvas authoring**. A `LinearSource` adapter is a follow-up PR.
- Hot-reload of a running workflow's definition mid-execution (Symphony spec allows this; we ship cold-reload first, hot-reload as a follow-up). New runs pick up the latest version; in-flight runs keep the version they started with.
- Multi-tenant isolation, untrusted-input filtering, prompt-injection defenses (upstream Symphony explicitly defers these; uClaw inherits the same trust posture: "trusted local environment").
- Distributed execution. All nodes run in the local process under one `SymphonyService`. Sharding across machines is a separate program.
- Replacing or wrapping `AppRuntimeService`. Symphony is a **third runtime**, not a refactor of the second.

### 1.3 Success criteria (verifiable)

- `cd src-tauri && cargo build` passes with all symphony modules compiled in.
- `cd src-tauri && cargo test --lib symphony` exercises: workflow create/list/delete; node-run lifecycle state machine; per-run + per-day cost gates; stall detection; restart reconciliation; persistence schema idempotency.
- `cd ui && npx tsc --noEmit` passes with the new `tab.type === 'symphony'` branch.
- `cd ui && npm test -- --run symphony` exercises: canvas mounts, atoms wire to IPC, node status transitions render, edge animation respects node state.
- A fresh canvas with two nodes (`A → B`) authored in the UI, with a manual "Run" trigger, produces two `agent_sessions` rows linked through `symphony_node_runs`, both with persisted transcripts, both with `cost_records` entries, and a single `symphony_runs.outcome = 'completed'` row.

## 2. Symphony spec alignment

Mapping each upstream Symphony concept (per [SPEC.md](https://github.com/openai/symphony/blob/main/SPEC.md), verified via the architecture deep-dive at verdent.ai and the betterstack walkthrough) onto a uClaw construct. The shape is preserved; the substrate is replaced.

| Symphony concept                  | uClaw realization                                                                             | Source of truth                                                                              |
| --------------------------------- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Orchestrator (Elixir/OTP daemon)  | `SymphonyService: ManagedService` in `services/`                                              | `src-tauri/src/services/types.rs`                                                            |
| Per-issue workspace (git clone)   | Per-node working dir `~/.uclaw/symphony/<run_id>/<node_id>/` (clone optional, scratch default) | new — under `WorkspaceRoot` derived from `~/.uclaw/`                                        |
| WORKFLOW.md (YAML + prompt body)  | `symphony_workflows.definition_yaml` + `definition_md`, with import/export                    | new schema (§8); editable from canvas (visual) or raw markdown panel                          |
| IssueRecord (normalized issue)    | `SymphonyNode` row (`id, kind, prompt_template, inputs_json, deps_json, retry_policy_json`)   | new schema (§8.2)                                                                            |
| Coding agent (Codex subprocess)   | One `HeadlessDelegate` per node invocation, driving `run_agentic_loop` once                   | `src-tauri/src/agent/headless.rs`                                                            |
| Active / terminal states          | `NodeStatus = Pending \| Ready \| Running \| Stalled \| Succeeded \| Failed \| Cancelled`     | new enum in `symphony/protocol/types.rs`; persisted in `symphony_node_runs.status`            |
| Polling cadence + tick loop       | `SymphonyService` event loop: `tokio::select!` on (IPC trigger, scheduler tick, node-done, stall-deadline). No external polling required. | new — analogous to `AppRuntimeService::execute_run` pipeline                                  |
| Retry backoff                     | `delay = min(10_000 * 2^(attempt-1), max_retry_backoff_ms)`, default max 300s — verbatim from spec | new `retry::backoff` helper in `symphony/runtime/`; mirrors `src-tauri/src/agent/retry/backoff.rs` |
| Stall detection (`stall_timeout_ms`) | Per-node heartbeat updated in `LoopDelegate::on_usage`; deadline checker in service tick    | new — sits next to the existing LLM-layer `STREAM_STALL_TIMEOUT = 45s` from `llm/stream_error.rs` |
| Single in-memory authoritative state | `RwLock<HashMap<RunId, RunActor>>` inside `SymphonyService`                                  | new — same shape as `AppRuntimeService.semaphores` / `attached`                              |
| Durable state (Linear)            | SQLite (`symphony_runs`, `symphony_node_runs`)                                                | new V33 migration                                                                            |
| Hooks `after_create` / `after_run` | Per-node `after_create_command` + `after_run_command` strings, executed via `tools/builtin/shell.rs` machinery under the node's `SafetyManager` policy | new — fields on `SymphonyNode`, executed by `SymphonyExecutor::run_node`                      |
| Proof-of-work (CI gate)           | Per-node "completion gate": a `CompletionGate` that the agent posts to via the existing `request_escalation` / `report_to_user` tools, or via a new `symphony_handoff` tool (§5.4). The merge decision (workflow `outcome = succeeded`) is the AND of all leaf gates. | reuses `automation::runtime::CompletionGate`                                                  |
| Hot-reload of WORKFLOW.md         | **Deferred** — cold reload on next run only (§1.2). Schema is versioned (`symphony_workflow_versions`) so the substrate is ready when we lift this. | new                                                                                          |
| OTP supervision tree              | One `tokio::spawn` per node, with `JoinHandle` owned by the run actor; panics caught at the spawn boundary and reported as `NodeStatus::Failed`. Crash isolation is per-node by construction. | new                                                                                           |
| Per-spec semaphore                | Per-workflow semaphore (`max_concurrent_nodes`, default 4) + global `SymphonyConfig.max_concurrent_runs` (default 2). | mirrors `AppRuntimeService::semaphores`                                                       |
| Trust posture is implementation-defined | uClaw's existing `SafetyManager` and `PendingApprovals` machinery is the answer. Symphony introduces no new trust model. | `src-tauri/src/safety/`                                                                       |

The one place Symphony's spec doesn't map 1:1 is the **issue tracker as durable state** assumption. uClaw replaces it with SQLite because (a) the desktop app already has a SQLite source of truth for every other domain, (b) the default usage is manual canvas authoring with no external tracker, and (c) when a `LinearSource` adapter ships later, it can be additive — it imports issues into `symphony_nodes` rows; the canonical state remains local.

## 3. Architecture

### 3.1 Module layout

The new code clusters in two locations, sized like the existing `automation/` and `agent/` modules.

```
src-tauri/src/
├── symphony/
│   ├── mod.rs                     # re-exports
│   ├── manager.rs                 # SymphonyManager — CRUD over workflows
│   ├── memubot_config.rs (extend) # SymphonyConfig appended in memubot_config.rs
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── types.rs               # NodeStatus, RunStatus, NodeKind, RetryPolicy
│   │   ├── parse.rs               # WORKFLOW.md (YAML front matter) parser
│   │   └── normalize.rs           # SymphonyWorkflowDef ↔ DB row conversions
│   ├── runtime/
│   │   ├── mod.rs                 # re-exports
│   │   ├── service.rs             # SymphonyService: ManagedService impl
│   │   ├── executor.rs            # SymphonyExecutor — owns one RunActor per run
│   │   ├── run_actor.rs           # spawn/await per-node tasks; reconcile DB state
│   │   ├── node_run.rs            # adapts HeadlessDelegate per node; cost cap; heartbeat
│   │   ├── stall.rs               # stall-deadline checker; cancellation
│   │   ├── cost.rs                # per-run + per-day caps; reuses cost_store
│   │   ├── retry.rs               # backoff formula identical to spec
│   │   └── recovery.rs            # restart reconciliation
│   ├── sources/                   # (Phase 2) — Linear/GitHub/manual triggers
│   │   ├── mod.rs
│   │   ├── manual.rs              # default: triggered by Tauri command
│   │   └── linear.rs (Phase 2)    # adapter
│   └── tools/                     # (optional Phase 2)
│       ├── mod.rs
│       └── symphony_handoff.rs    # `record_handoff` tool: signal node completion
│
└── tauri_commands.rs (extend)     # symphony_* commands + invoke_handler entries

ui/src/
├── components/
│   └── symphony/
│       ├── index.ts
│       ├── SymphonyCanvas.tsx          # top-level view, registered as tab.type === 'symphony'
│       ├── canvas/
│       │   ├── WorkflowCanvas.tsx      # @xyflow/react ReactFlow root
│       │   ├── NodeCard.tsx            # custom node renderer (status pill, cost chip, runtime)
│       │   ├── EdgeWire.tsx            # custom edge with state-aware animation
│       │   ├── CanvasToolbar.tsx       # run / pause / cancel / fit-view / export
│       │   ├── PaletteSidebar.tsx      # left: drag node templates
│       │   ├── InspectorPanel.tsx      # right: selected node prompt + retry + permissions
│       │   └── MinimapLegend.tsx
│       ├── RunHistoryPanel.tsx         # bottom: per-run rollup of leaves' costs / duration
│       └── WorkflowMarkdownEditor.tsx  # raw WORKFLOW.md editor (Phase 1, behind a "Raw" tab)
└── atoms/
    ├── symphony.ts                     # workflows, runs, nodes, current ids
    └── symphony-canvas.ts              # canvas viewport / selection / palette state
```

### 3.2 Boot sequence

`main.rs` Stage 3 picks up exactly one new line right after `AppRuntimeService` registration (see `src-tauri/src/main.rs:271-278`):

```rust
// SymphonyService — third parallel runtime alongside Agent + Automation.
if memubot_config.symphony.enabled {
    let symphony_svc = uclaw_core::symphony::SymphonyService::new(
        db.clone(),
        infra.clone(),
        provider_service.clone(),
        safety_manager.clone(),
        memory_graph.clone(),
        proactive_bus.clone(),     // if proactive is enabled
        cost_store.clone(),
        app_handle.clone(),
        memubot_config.symphony.clone(),
    );
    service_manager.register(symphony_svc).await;
    tracing::info!("[Stage 3] SymphonyService registered");
}
```

Stage 4's `service_manager.start_all()` then drives `SymphonyService::start()`, which:

1. Loads workflows from `symphony_workflows`.
2. Reconciles in-flight runs: every `symphony_runs.status IN ('queued','running')` becomes a fresh `RunActor`, its node statuses are recomputed (rows in `symphony_node_runs.status IN ('ready','running')` whose `last_heartbeat_ms` is older than `stall_timeout_ms` become `'stalled'`).
3. Starts a tokio task that owns the tick loop: drain `manual_trigger_rx`, drive scheduler ticks (per-workflow cron, when added in Phase 2), check stall deadlines.

### 3.3 The execute_run pipeline (analog of `AppRuntimeService::execute_run`)

This is the per-run lifecycle, modeled on the comment block at the top of `src-tauri/src/automation/runtime/service.rs`. A Symphony run goes through:

```
TRIGGER  →  resolve workflow def + version
         →  PERMIT (global concurrency semaphore — SymphonyConfig.max_concurrent_runs)
         →  COST CAP CHECK (per-day; rejection becomes RunStatus::SkippedQuotaExceeded)
         →  CREATE symphony_runs row + RunActor
         →  topological-schedule loop:
              while not done:
                  ready_nodes = nodes whose deps are all Succeeded
                  for each ready node within per-workflow concurrency cap:
                      PER_NODE_PERMIT  →  build_headless_delegate(node)
                                       →  spawn_node_task(node, delegate)
                                            ├─ persist transcript on completion (run_session.rs pattern)
                                            ├─ emit InfraEvent + symphony:node_update IPC
                                            └─ update symphony_node_runs row
                  await first node completion or stall_deadline
                  on failure: apply retry policy; on exhausted retries: mark Failed
                              and trigger workflow's failure_mode (Abort | Continue | Branch)
         →  on done: update symphony_runs.outcome, prune old runs per retention policy
```

This is structurally **the same shape** as `AppRuntimeService::execute_run`'s pipeline. The reader can copy that file's design and substitute `spec` → `workflow`, `activity` → `run`, single `HeadlessDelegate` → N `HeadlessDelegate`s scheduled by deps.

### 3.4 Reuse map

For each major piece of new behavior, the table below says **where to reach for existing code** rather than write fresh.

| Need                          | Reach for                                                                  |
| ----------------------------- | -------------------------------------------------------------------------- |
| Run an LLM-driven step        | `agent::headless::HeadlessDelegate` + `agent::agentic_loop::run_agentic_loop` |
| Persist a step's transcript   | `automation::runtime::run_session::persist_transcript`                     |
| Per-step home space           | New `SYMPHONIES_SPACE_ID = "symphonies"` + `ensure_symphonies_space()` (a 30-line clone of `automation::runtime::run_session::ensure_automations_space`) |
| Per-step session row + chain  | `automation::runtime::run_session::create_run_session` adapted to set `metadata.origin = "symphony:<node_id>"` |
| Cost accounting               | `cost_store::record` + `cost_store::monthly_total`; new per-day `symphony_day_total` helper |
| Per-step + per-day caps       | `automation::runtime::cost::CostCapState` + `CostCapConfig` (the type is generic in shape; clone the per-spec_*_cost helpers into `symphony/runtime/cost.rs`) |
| Tool set                      | `agent::tools::tool::ToolRegistry` — same full base tool set every node sees |
| Tool approvals                | `safety::SafetyManager` + `app.pending_approvals` — no Symphony-specific surface |
| LLM provider resolution       | `providers::service::ProviderService` (resolve at `RunActor` start, reuse across nodes) |
| Event bus                     | `infra::InfraService` — new event types only if existing ones are insufficient (§4) |
| Long-term memory              | `memory_graph::MemoryGraphStore` — workflow's prior runs ingested into the graph by a `symphony_run_completed` proactive scenario (Phase 2) |
| Streaming UI updates          | `infra::InfraService::publish_*` + Tauri `app_handle.emit("symphony:...", payload)`; mirrors `agent:turn_cost` emission in `agent::dispatcher::emit_turn_cost` |
| Markdown / WORKFLOW.md parse  | `serde_yml` (already a dep via Humane parser at `automation/protocol/parse.rs:50`) |
| Migration scaffolding         | `db::migrations` — append V33 with the tolerant-per-statement pattern used by V14, V22, V24, V25, V26, V27 |

This is **the single most important point in this design**: Symphony adds glue, not engine. The only piece of genuinely new Rust is the DAG scheduler in `symphony/runtime/run_actor.rs`. Everything else is a thin orchestrator on top of code that already exists, tests, and ships.

## 4. IPC and events

### 4.1 Tauri events emitted to the canvas

Single-channel-per-event-type, payload is JSON, listener lives in `SymphonyCanvas.tsx`:

| Event name                    | Payload                                                                                                 | When                                                                 |
| ----------------------------- | ------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `symphony:run_started`        | `{runId, workflowId, startedAt}`                                                                        | `RunActor::spawn` succeeded; row inserted                            |
| `symphony:node_update`        | `{runId, nodeId, status, attempt, lastHeartbeatMs, costUsd?, durationMs?, error?}`                      | Every state transition + on each `on_usage` heartbeat                |
| `symphony:node_log`           | `{runId, nodeId, role, content, blockKind, idx}`                                                        | Per assistant/tool/user message persisted; throttled to 4Hz in canvas |
| `symphony:run_completed`      | `{runId, outcome, durationMs, totalCostUsd, succeeded, failed, cancelled}`                              | `RunActor` exits                                                     |
| `symphony:workflow_updated`   | `{workflowId, version}`                                                                                 | Workflow saved via `symphony_save_workflow`                          |
| `symphony:cost_cap_hit`       | `{runId, nodeId?, scope: 'per_run' \| 'per_day', usd, capUsd}`                                          | Cost guard tripped                                                   |

Note the deliberate parallel with `agent:turn_cost` (`src-tauri/src/agent/dispatcher.rs:614`) — the canvas should listen for **both** the broad `agent:turn_cost` (for cross-runtime cost telemetry) and the Symphony-specific events.

### 4.2 InfraService event types

Two new variants on `infra::types::InfraEventType` (existing enum at `src-tauri/src/infra/types.rs:10-39`):

```rust
pub enum InfraEventType {
    // ... existing variants ...
    /// A symphony run finished (success, failure, or cancellation).
    SymphonyRunCompleted,
    /// A symphony node finished one attempt.
    SymphonyNodeCompleted,
}
```

Why only two: every node-level fine-grained event already has a home in the existing event set. `ToolExecuted` covers per-tool calls inside a node. `LoopCompleted` / `LoopFailed` cover the underlying agentic loop's outcome. `MemoryExtracted` / `SkillLearned` cover post-run learning. The two new variants exist so the **proactive subsystem** can subscribe to "a Symphony workflow finished" without polling.

### 4.3 Tauri command surface

New commands in `tauri_commands.rs`, **each must also be listed in `main.rs:409` `invoke_handler!`** (see CLAUDE.md Part 1, "Adjacent edits that look like scope creep but aren't"):

```
symphony_list_workflows
symphony_get_workflow(workflowId) -> SymphonyWorkflowDetail
symphony_save_workflow(input: SaveWorkflowInput) -> { workflowId, version }
symphony_delete_workflow(workflowId)
symphony_import_workflow_md(yamlMd: String) -> SymphonyWorkflowDetail
symphony_export_workflow_md(workflowId) -> String

symphony_list_runs(workflowId?) -> Vec<SymphonyRunRow>
symphony_get_run(runId) -> SymphonyRunDetail   # includes nodes + edges + costs
symphony_trigger_run(workflowId, inputs_json?) -> runId
symphony_cancel_run(runId)
symphony_retry_failed_nodes(runId)              # opt-in: only failed nodes
symphony_get_node_session_id(runId, nodeId) -> sessionId  # opens AgentView for node's transcript

symphony_get_service_health() -> ServiceHealth  # the standard ManagedService snapshot
```

All return types are typed via `lib/tauri-bridge.ts`. Convention matches existing automation commands (`list_automations_humane`, `get_automation_activity`).

## 5. Backend deep dive

### 5.1 `SymphonyService`

Implements `ManagedService`. Same shape as `AppRuntimeService`:

```rust
pub struct SymphonyService {
    db: Arc<StdMutex<Connection>>,
    infra: Arc<InfraService>,
    provider_service: Arc<ProviderService>,
    safety: Arc<SafetyManager>,
    memory_graph: Arc<MemoryGraphStore>,
    cost: Arc<CostCapState>,                            // per-day cap, shared across runs
    config: SymphonyConfig,
    app_handle: tauri::AppHandle,

    /// Authoritative in-memory state, owned by the orchestrator (Symphony spec MUST).
    runs: Arc<RwLock<HashMap<RunId, Arc<RunActor>>>>,

    /// Global concurrency limiter across all runs.
    global_run_sem: Arc<Semaphore>,

    /// Manual-trigger channel; populated by `symphony_trigger_run`.
    trigger_tx: mpsc::Sender<TriggerCmd>,
    trigger_rx: Mutex<Option<mpsc::Receiver<TriggerCmd>>>,

    status: Arc<StdMutex<ServiceStatus>>,
    started_at: Arc<StdMutex<Option<Instant>>>,
}

#[async_trait]
impl ManagedService for SymphonyService {
    fn name(&self) -> &str { "SymphonyService" }
    async fn start(&self) -> anyhow::Result<()> { /* reconcile + spawn tick loop */ }
    async fn stop(&self) -> anyhow::Result<()> { /* signal each RunActor to gracefully halt; await with timeout */ }
    fn status(&self) -> ServiceStatus { self.status.lock().unwrap().clone() }
    fn health(&self) -> ServiceHealth { /* uptime + per-status counts */ }
}
```

Concurrency: every `RunActor` is a `tokio::spawn`ed task. On `stop()`, the service flips status to `Stopping`, sends `LoopSignal::Cancel` (via the existing `agent::types::LoopSignal` enum) to each in-flight node, awaits with a 10s budget per run, then forces `JoinHandle::abort()` on stragglers. The `STOP_TIMEOUT_SECS = 5` in `services::manager` covers the outer service-stop; nodes get the extra 5s because they may need to flush a transcript.

### 5.2 `RunActor`

```rust
pub struct RunActor {
    run_id: RunId,
    workflow: Arc<SymphonyWorkflowDef>,
    state: Arc<RwLock<RunState>>,             // node statuses, per-node retry counters
    per_workflow_sem: Arc<Semaphore>,         // local cap (`max_concurrent_nodes`)
    cancel: CancellationToken,                // tokio_util::sync::CancellationToken
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl RunActor {
    pub fn spawn(...) -> Arc<Self> { /* ... */ }

    async fn run_loop(self: Arc<Self>) {
        loop {
            select! {
                _ = self.cancel.cancelled() => { self.cancel_all_nodes().await; break }
                ready = self.next_ready_node() => {
                    let permit = self.per_workflow_sem.clone().acquire_owned().await;
                    self.spawn_node(ready, permit);
                }
                done = self.next_completed_node() => {
                    self.apply_node_outcome(done).await;
                    if self.is_terminal() { break; }
                }
                _ = sleep(self.stall_tick()) => { self.check_stalls().await; }
            }
        }
        self.finalize_run().await;
    }
}
```

### 5.3 Per-node executor: `node_run.rs`

The bridge between Symphony and uClaw's existing agent loop. **Reuses** `HeadlessDelegate` end-to-end:

```rust
pub async fn execute_node(
    run_id: &RunId,
    node: &SymphonyNode,
    upstream_outputs: &NodeOutputMap,
    deps: &NodeExecutionDeps,
) -> NodeOutcome {
    // 1. Prepare per-node workspace dir.
    let workspace = ensure_node_workspace(run_id, &node.id, &deps.workspace_root)?;

    // 2. Resolve provider + model (workflow-level override, else workflow default, else app default).
    let (llm, model) = deps.provider_service.resolve_for_node(&node).await?;

    // 3. Build the per-node tool registry (full base set; opt-in to extras via node.tools).
    let tools = deps.tool_registry_factory.build(&node)?;

    // 4. Construct HeadlessDelegate — identical to automation's construction.
    let delegate = HeadlessDelegate {
        spec_id: node.workflow_id.0.clone(),                        // workflow id in the spec_id slot
        activity_id: run_id.0.clone(),                              // symphony run_id in activity slot
        session_id: create_node_session(&deps.db, run_id, &node.id)?, // new agent_session row
        permissions: build_permission_set(&node.permissions),
        memory: deps.memory_store.clone(),
        db: deps.db.clone(),
        gate: Arc::new(Mutex::new(None)),
        auto_continue: node.auto_continue.unwrap_or_default(),
        llm,
        model,
        tools,
        cost: Arc::new(CostCapState::new(node.cost_cap.into())),
        workspace_root: workspace,
        app_handle: Some(deps.app_handle.clone()),
        channel_manager: deps.channel_manager.clone(),
        reply_handle: None,
        streaming_handle: Some(Arc::new(SymphonyHeartbeatSink {
            run_id: run_id.clone(),
            node_id: node.id.clone(),
            app: deps.app_handle.clone(),
            heartbeat: deps.heartbeat.clone(),
        })),
        system_prompt_override: Some(render_node_prompt(node, upstream_outputs)?),
    };

    // 5. Build the ReasoningContext seed and drive the loop.
    let mut reason_ctx = build_reasoning_context_for(&node, upstream_outputs);
    let cfg = AgenticLoopConfig {
        max_iterations: node.max_iterations.unwrap_or(deps.config.default_max_iterations),
        ..Default::default()
    };
    let outcome = run_agentic_loop(&delegate, &mut reason_ctx, &cfg).await;

    // 6. Persist transcript (same call automation uses).
    persist_transcript(&deps.db.lock().unwrap(), &delegate.session_id, &reason_ctx.messages)?;

    // 7. Run `after_run_command` under SafetyManager if present.
    if let Some(cmd) = &node.after_run_command {
        run_post_hook(cmd, &workspace, &deps.safety).await?;
    }

    NodeOutcome::from_loop_outcome(outcome, delegate.cost.total_usd())
}
```

Three points worth calling out:

- The `SymphonyHeartbeatSink` is a tiny new `StreamingHandle` impl that piggybacks on `HeadlessDelegate`'s existing IM-streaming hook. Every partial-text update triggers `heartbeat.touch(node_id)` and emits a throttled `symphony:node_log` event. This is how we get stall detection for free.
- `spec_id` and `activity_id` are repurposed as `workflow_id` and `run_id` respectively. The transcripts still land in `agent_messages` keyed by `session_id`, exactly as automation runs do, so the existing Agent UI can open any Symphony node's transcript without modification.
- `after_run_command` is the Symphony spec's hook. It runs through the **same** shell tool implementation already used by `agent::tools::builtin::shell::Shell` so the safety / sandbox story doesn't fork.

### 5.4 Optional Symphony-specific tools (Phase 2)

`symphony/tools/symphony_handoff.rs` exposes one tool:

```yaml
name: record_handoff
description: |
  Record this node's output for downstream consumers and mark this node as succeeded.
  Use this when the work is done and you want the next node(s) to proceed.
input_schema:
  type: object
  required: [output]
  properties:
    output: { type: string, description: "Markdown summary handed to downstream nodes" }
    artifacts: { type: array, items: { type: string }, description: "Optional file paths" }
    confidence: { type: number, minimum: 0, maximum: 1 }
```

It's schema-only on the LLM side (like `HumaneToolSchema` in `automation/runtime/service.rs:65`) — the `RunActor` dispatches it before the registry fallthrough by intercepting the tool call name. Default workflows that don't need explicit handoff signaling can omit it; the node reports completion via the standard `report_to_user` / loop exit instead.

### 5.5 Recovery

`SymphonyService::start()` runs reconciliation **before** opening the trigger channel:

```rust
async fn reconcile(&self) -> Result<()> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let stall_horizon = now_ms - self.config.stall_timeout_ms as i64;

    // Mark stalled node-runs whose last heartbeat is older than the horizon.
    self.db.lock().unwrap().execute(
        "UPDATE symphony_node_runs
         SET status = 'stalled', updated_at = ?1
         WHERE status IN ('running','ready')
           AND COALESCE(last_heartbeat_ms, started_at_ms) < ?2",
        params![now_ms, stall_horizon],
    )?;

    // For each run whose status is queued/running, build a RunActor whose
    // state mirrors the DB. Resume nodes that are 'ready' or 'pending'.
    for row in self.load_active_runs()? {
        let actor = RunActor::resume(row, self.deps()).await?;
        self.runs.write().await.insert(actor.run_id.clone(), actor);
    }
    Ok(())
}
```

Recovery rules, in plain English:

- A `queued` run resumes from the start.
- A `running` run resumes node-by-node: any node that was `running` becomes `stalled` (its agentic loop crashed when the app died), and its retry policy kicks in on next tick.
- A node with `attempt >= max_attempts` becomes `failed`. The workflow's `failure_mode` decides whether the run continues with other branches or aborts.
- The original `agent_session` for a stalled node is preserved (so the partial transcript survives in `agent_messages`); the retry creates a **new** session row chained via `metadata.prev_run_session_id` — same chain that automation already uses (`automation::runtime::run_session::create_run_session:50-57`).

### 5.6 Cost guardrails

Two layers on top of the existing `cost_store`:

- **Per-node**: `node.cost_cap_usd` (default `SymphonyConfig.default_per_node_cap_usd`). Enforced inside `HeadlessDelegate::before_llm_call` via its existing `cost.per_run_exceeded()` check. No new code path.
- **Per-run**: sum of node costs against `workflow.per_run_cost_cap_usd` (default `SymphonyConfig.per_run_cost_cap_usd`). Enforced by `RunActor` before each new node spawn. On trip, remaining ready nodes are marked `cancelled` (reason: `cost_cap_exceeded`).
- **Per-day**: `SymphonyConfig.per_day_cost_cap_usd`. Enforced at `symphony_trigger_run` time and rechecked before each new run. Counted via a new `cost_store::symphony_day_total(since_ms)` helper that filters `cost_records` by `metadata.origin LIKE 'symphony:%'` — the metadata is set by `create_node_session` mirroring automation's pattern.

All caps respect the existing `monthly_budget_usd` setting and emit `budget:threshold` exactly like `agent::dispatcher::emit_turn_cost` does, so a Symphony run contributes to the same global cost dashboard.

## 6. Frontend deep dive

### 6.1 App-shell wiring

Three small surgical changes:

1. `ui/src/atoms/app-mode.ts`: extend the union.
   ```ts
   export type AppMode = 'chat' | 'agent' | 'symphony'
   ```
2. `ui/src/atoms/tab-atoms.ts`: extend `TabType` to include `'symphony'`.
3. `ui/src/components/tabs/TabContent.tsx`: add a branch.
   ```tsx
   if (tab.type === 'symphony') {
     return (
       <TabErrorBoundary key={tab.sessionId} sessionId={tab.sessionId}>
         <SymphonyCanvas workflowId={tab.sessionId} />
       </TabErrorBoundary>
     )
   }
   ```
4. `ui/src/components/app-shell/ModeSwitcher.tsx`: add a third entry to the `modes` array; widen the slider from `w-[calc(50%-4px)]` to `w-[calc(33.333%-4px)]`. Restoration logic in `restoreSession` generalizes naturally — replace the `isChatMode` boolean with a `switch (targetMode)` over the three options.

`tab.sessionId` for a Symphony tab is the **workflow id** (so multiple tabs can show different workflows of the same workflow type if needed; identical to how Chat tabs hold a conversation id). The current run is held in a separate atom (`currentSymphonyRunIdAtom`) keyed by workflow id, mirroring `currentAgentSessionIdAtom`.

### 6.2 Canvas (`SymphonyCanvas` + `WorkflowCanvas`)

`@xyflow/react` (≈ 220kb gzipped, lazy-chunked via `vite.config.ts` manualChunks so it doesn't bloat the initial load). The canvas owns a `ReactFlow` instance whose nodes and edges are derived from atoms; selection and viewport are stored in `symphony-canvas.ts` atoms (localStorage-backed via `atomWithStorage`, scoped by workflow id).

Three sub-views, switchable via a top tab strip:

- **Design** — author the workflow; drag nodes from `PaletteSidebar`, connect with edges, edit the selected node in `InspectorPanel`. Auto-save with a 500ms debounce → `symphony_save_workflow`.
- **Run** — replaces the design grid with a status-colored version of the same graph. Nodes show: status pill (Pending/Running/Succeeded/Failed/…), cumulative cost chip, current iteration / max-iterations, elapsed time. Clicking a node opens an inline drawer with its live transcript stream (subscribed to `symphony:node_log`). Edges animate when their source node is `Running`.
- **Raw** — `WorkflowMarkdownEditor` over `definition_md`, for users who prefer text. CodeMirror with YAML + Markdown syntax (deps already in `package.json`).

Theming uses the existing tokens — `bg-popover`, `text-muted-foreground`, `bg-accent`, `border-border`. **No hardcoded colors** (CLAUDE.md Part 1, "Theming" — hardcoded `bg-zinc-900` breaks under `warm-paper` / `forest-*` themes). Node status colors come from a token map:

```ts
const NODE_STATUS_TOKENS = {
  pending:   'bg-muted/50 text-muted-foreground border-border',
  ready:     'bg-accent/40 text-accent-foreground border-accent',
  running:   'bg-primary/15 text-primary border-primary animate-pulse',
  succeeded: 'bg-green-500/15 text-green-600 dark:text-green-400 border-green-500/40',
  failed:    'bg-destructive/15 text-destructive border-destructive',
  stalled:   'bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/40',
  cancelled: 'bg-muted text-muted-foreground border-border',
}
```

The `green-500` / `amber-500` references are the same pattern `AutomationHub.tsx:32-37` already uses for status icons, so we're not introducing a new convention.

### 6.3 IPC subscription pattern

Inside `SymphonyCanvas`:

```ts
useEffect(() => {
  const unlistens: Promise<UnlistenFn>[] = []
  unlistens.push(listen('symphony:node_update', e => updateNode(e.payload as NodeUpdate)))
  unlistens.push(listen('symphony:node_log', e => appendLog(e.payload as NodeLog)))
  unlistens.push(listen('symphony:run_started', e => upsertRun(e.payload)))
  unlistens.push(listen('symphony:run_completed', e => finalizeRun(e.payload)))
  unlistens.push(listen('symphony:cost_cap_hit', e => toastCostCap(e.payload)))
  // Cross-runtime: surface per-turn cost on the active node.
  unlistens.push(listen('agent:turn_cost', e => attributeCost(e.payload)))
  return () => { unlistens.forEach(p => p.then(fn => fn())) }
}, [workflowId])
```

Same pattern AgentView already uses, so anyone who's read `AgentView.tsx` will recognize this immediately.

### 6.4 New atoms

`ui/src/atoms/symphony.ts`:
```ts
export const symphonyWorkflowsAtom        = atom<SymphonyWorkflowRow[]>([])
export const currentSymphonyWorkflowIdAtom = atom<string | null>(null)
export const symphonyRunsByWorkflowAtom    = atom<Record<string, SymphonyRunRow[]>>({})
export const currentSymphonyRunIdAtom      = atom<string | null>(null)
export const symphonyNodesByRunAtom        = atom<Record<string, SymphonyNodeRunRow[]>>({})
export const symphonyEdgesByWorkflowAtom   = atom<Record<string, SymphonyEdgeRow[]>>({})
```

`ui/src/atoms/symphony-canvas.ts` — viewport, selection, palette open state. `atomWithStorage` scoped per workflow id so the user's zoom/pan survives across navigation.

### 6.5 Vite chunking

Append to `vite.config.ts` manualChunks:

```ts
if (id.includes('@xyflow/react') || id.includes('reactflow')) return 'xyflow'
```

So the canvas chunk is lazy-loaded only when a Symphony tab is opened.

## 7. Configuration

`MemubotConfig.symphony: SymphonyConfig` (new), defaults below; persisted in `~/.uclaw/memubot_config.json` like the rest:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SymphonyConfig {
    /// Whether SymphonyService is registered + started.
    pub enabled: bool,                          // default true
    /// Max concurrent in-flight runs across all workflows.
    pub max_concurrent_runs: usize,             // default 2
    /// Default per-workflow concurrency for ready nodes.
    pub default_max_concurrent_nodes: usize,    // default 4
    /// Per-node default cost cap.
    pub default_per_node_cost_cap_usd: f64,     // default 1.00
    /// Per-run default cost cap.
    pub default_per_run_cost_cap_usd: f64,      // default 5.00
    /// Daily cap across all Symphony runs.
    pub per_day_cost_cap_usd: f64,              // default 25.00
    /// How long without heartbeat before a node is considered stalled.
    pub stall_timeout_ms: u64,                  // default 180_000 (3 min)
    /// Default max iterations for an agentic loop inside a node.
    pub default_max_iterations: usize,          // default 30
    /// Default max retry backoff cap (Symphony spec formula).
    pub max_retry_backoff_ms: u64,              // default 300_000 (5 min, spec default)
    /// Per-workflow number of recent runs to retain.
    pub retention_runs_per_workflow: u32,       // default 50
}
```

Note the parallel with `AutomationConfig` (`src-tauri/src/memubot_config.rs:118`): same five concerns (per-run cap, per-day cap, retention, max iterations, plus Symphony-specific stall + concurrency). New code clones that struct's defaults pattern.

## 8. Database schema

### 8.1 Migration number and CLAUDE.md correction

CLAUDE.md Part 2's *Active migration registry* lists V26 as "in progress" and stops there. The actual `db/migrations.rs` already runs through **V32 + V32b**. The next free version is therefore **V33**. As part of this PR, CLAUDE.md's table will be updated to reflect V27 (system_prompts), V28 (system_prompt_versions), V29 (compaction support), V30 (fragment tables), V31 (memory_fts trigram), V32 (IM channel tables), V32b (automation_specs IM columns), V33 (Symphony — this PR). This corrects a documentation-vs-reality drift that has accumulated across the last seven merged PRs.

### 8.2 V33 schema (`SQL_V33_SYMPHONY`)

Tolerant-per-statement pattern (matches V25, V26 in `migrations.rs:1530-1545`). Each `CREATE TABLE IF NOT EXISTS` and each `CREATE INDEX IF NOT EXISTS` is idempotent.

```sql
-- Workflow definition (one row per workflow; versions kept in symphony_workflow_versions).
CREATE TABLE IF NOT EXISTS symphony_workflows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    space_id        TEXT,
    current_version INTEGER NOT NULL DEFAULT 1,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE SET NULL
);

-- Immutable snapshots of each workflow version. New runs pin a version
-- so in-flight runs survive workflow edits (cold-reload semantics §1.2).
CREATE TABLE IF NOT EXISTS symphony_workflow_versions (
    workflow_id     TEXT NOT NULL,
    version         INTEGER NOT NULL,
    definition_yaml TEXT NOT NULL,        -- canonical YAML (front matter only)
    definition_md   TEXT NOT NULL,        -- full WORKFLOW.md (yaml + body)
    nodes_json      TEXT NOT NULL,        -- normalized SymphonyNode[]
    edges_json      TEXT NOT NULL,        -- normalized SymphonyEdge[]
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, version),
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);

-- One row per run.
CREATE TABLE IF NOT EXISTS symphony_runs (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    workflow_version INTEGER NOT NULL,
    trigger_kind    TEXT NOT NULL,         -- 'manual' | 'scheduled' | 'linear' | ...
    trigger_payload_json TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL,         -- 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'quota_exceeded'
    outcome         TEXT,                  -- 'succeeded' | 'partial' | 'failed' | NULL
    inputs_json     TEXT NOT NULL DEFAULT '{}',
    outputs_json    TEXT,                  -- merged leaf outputs
    total_cost_usd  REAL NOT NULL DEFAULT 0,
    error_text      TEXT,
    queued_at       INTEGER NOT NULL,
    started_at      INTEGER,
    completed_at    INTEGER,
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_symphony_runs_workflow ON symphony_runs(workflow_id, queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_symphony_runs_status ON symphony_runs(status);

-- One row per node per attempt. Multiple rows for retried nodes.
CREATE TABLE IF NOT EXISTS symphony_node_runs (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    node_id         TEXT NOT NULL,         -- stable within the workflow version
    attempt         INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL,         -- pending|ready|running|stalled|succeeded|failed|cancelled
    session_id      TEXT,                  -- agent_sessions.id
    cost_usd        REAL NOT NULL DEFAULT 0,
    iterations      INTEGER NOT NULL DEFAULT 0,
    started_at_ms   INTEGER,
    last_heartbeat_ms INTEGER,
    completed_at_ms INTEGER,
    error_text      TEXT,
    output_json     TEXT,                  -- structured output for downstream nodes
    FOREIGN KEY (run_id) REFERENCES symphony_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_run ON symphony_node_runs(run_id, node_id);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_status ON symphony_node_runs(status);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_heartbeat ON symphony_node_runs(last_heartbeat_ms);

-- Seed the 'symphonies' home space, idempotent.
INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('symphonies', 'Symphonies', '🎼', NULL, datetime('now'), datetime('now'));
```

There is **no** `symphony_nodes` / `symphony_edges` first-class table — they live inside `symphony_workflow_versions.nodes_json` / `edges_json`. This matches automation's choice to keep `humane_v1` spec body in `automation_specs.spec_json` rather than normalize it. The trade-off: structural queries over nodes ("how many workflows reference an HTTP fetch tool?") aren't index-accelerated. We can normalize later if usage demands; the V33 shape doesn't paint us into a corner.

### 8.3 FTS

Deferred. The Symphony canvas is the primary discovery surface; full-text search over node prompts can be added in a later V34 if usage demands. **Don't forget the V11/V12 FTS-backfill pattern** if/when we add it (CLAUDE.md Part 1, "FTS backfill").

## 9. Security and trust

uClaw inherits Symphony's "implementation-defined trust posture" by reusing:

- `SafetyManager` for every node's tool calls. Per-node `permissions` JSON (subset of automation's `Permission` enum) tightens the default. Risky tools route through `pending_approvals` → `approve_tool_call` Tauri command exactly as Chat/Agent runs do.
- `tauri.conf.json` CSP `connect-src` allow-list. **No new providers** are introduced by Symphony, so the CSP doesn't change (CLAUDE.md Part 1, "CSP + providers").
- `after_create_command` / `after_run_command` run through the existing `agent::tools::builtin::shell::Shell` machinery — including its workspace-bound cwd, command allow-listing, and the SafetyManager prompt-for-approval flow for destructive ops.

What Symphony **does not introduce**:

- Multi-tenant isolation. uClaw is single-user-local; the upstream Symphony spec defers this and so do we.
- Untrusted-input sanitization on workflow inputs. The Markdown body of a node prompt is rendered into the LLM prompt directly. We document this and surface it as a yellow banner when a workflow is imported from an untrusted source. A future content-scanning pass can plug in via `proactive::scenarios`.

## 10. Observability

Every state transition writes to three places:

1. **SQLite** — `symphony_runs.status`, `symphony_node_runs.status`. The source of truth for recovery.
2. **InfraService** — `SymphonyRunCompleted` / `SymphonyNodeCompleted` events for proactive subscribers (e.g. a future "review Symphony failures" scenario).
3. **Tauri IPC** — the canvas event names above (§4.1), used by the canvas for live updates and by the cost dashboard for cross-runtime rollups.

The existing `metrics` and `tracing` instrumentation in `observability/` automatically picks up Symphony — every node run flows through `run_agentic_loop`, which already emits `info` / `debug` lines tagged with `spec_id` and `activity_id`. The `spec_id` field will carry the workflow id and `activity_id` will carry the run id, which makes log greps work the same way they do for automation today.

## 11. Risks and mitigations

| Risk                                                                                   | Mitigation                                                                                                                            |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `@xyflow/react` adds ~220kb gzipped to the bundle.                                     | Lazy-load via Vite manual chunk; the chunk only enters the user's session when they open a Symphony tab.                              |
| Restart-during-node-run leaves SQLite + filesystem out of sync (workspace dir orphaned). | Workspace dirs are GC'd on Symphony service start: any `~/.uclaw/symphony/<run_id>/` whose `run_id` is absent or in a terminal state is deleted. |
| Per-day cost cap is global but the user expects per-workflow caps.                     | Phase 2 — extend `symphony_workflows` with `daily_cost_cap_usd` override; today's global is a safety net, not the only knob.          |
| Two open PRs both claim V33.                                                           | Migration registry in CLAUDE.md is updated **in the first commit** of this PR; reviewers can verify no conflict before merge.         |
| Hot-reload-of-running-workflow is the Symphony spec's "MUST attempt to adjust live behavior" — we're shipping cold reload first. | Versioned schema (`symphony_workflow_versions`) is in V33; in-flight runs pin a version. Hot-reload is a follow-up that swaps the pinned version atomically at safe points. |
| The DAG executor in `run_actor.rs` is genuinely new code; bugs here cascade.           | Heavy unit-test coverage of the scheduler in isolation (mock `HeadlessDelegate` like `automation/runtime/execute.rs` already does); explicit invariant checks in `is_terminal()` and `next_ready_node()`; property-test over generated random DAGs. |
| Symphony nodes can deadlock on circular deps.                                          | `symphony_save_workflow` rejects workflows whose `edges_json` produces a cycle; tested in `protocol::normalize::tests`.               |
| Existing CLAUDE.md migration table is stale.                                           | First commit corrects it (see §8.1). Reviewers can verify in isolation.                                                                |

## 12. Phased delivery

Phase 1 (this PR, see plan):
- V33 migration, `SymphonyConfig`, `SymphonyService` skeleton.
- Manual-trigger source only.
- Per-node `HeadlessDelegate` execution.
- Recovery on restart.
- Cost guardrails (per-node, per-run, per-day).
- Stall detection + retry.
- Canvas: Design + Run + Raw views with @xyflow/react.
- Tauri commands + invoke_handler entries.
- Unit tests in both crates.

Phase 2 (follow-ups, separate PRs):
- `LinearSource` / `GitHubIssueSource` adapters.
- `record_handoff` tool.
- Hot-reload of running workflows.
- Proactive scenario: "Symphony post-run review" (ingests completed run into MemoryGraph).
- FTS over node prompts (V34).
- Workflow templates marketplace.

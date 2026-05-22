# 01. Framework Design Comparison: jcode vs uClaw

Status: analysis document, no implementation changes.
Date: 2026-05-23
Scope: Rust backend framework design.

## Evidence

This report is based on static source inspection plus a refreshed GitNexus index for `uclaw-new`.

Primary uClaw evidence:

- `/Users/ryanliu/Documents/uclaw/BEHAVIOR.md`
- `/Users/ryanliu/Documents/uclaw/CONTEXT.md`
- `/Users/ryanliu/Documents/uclaw/docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/main.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/agentic_loop.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/dispatcher.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/tool.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/contracts.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/registries/mod.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/world/mod.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/infra/service.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/services/manager.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/safety/mod.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/mcp.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/gbrain/mod.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/memu/bridge.rs`

Primary jcode evidence:

- `/Users/ryanliu/Documents/jcode/Cargo.toml`
- `/Users/ryanliu/Documents/jcode/src/main.rs`
- `/Users/ryanliu/Documents/jcode/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/src/cli/dispatch.rs`
- `/Users/ryanliu/Documents/jcode/src/server.rs`
- `/Users/ryanliu/Documents/jcode/src/server/socket.rs`
- `/Users/ryanliu/Documents/jcode/src/agent.rs`
- `/Users/ryanliu/Documents/jcode/src/agent/turn_execution.rs`
- `/Users/ryanliu/Documents/jcode/src/agent/turn_streaming_broadcast.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/mod.rs`
- `/Users/ryanliu/Documents/jcode/src/safety.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-agent-runtime/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-provider-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-tool-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-protocol/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-session-types/src/lib.rs`

Commands used:

- `cargo metadata --format-version=1 --no-deps`
- `find ... -name Cargo.toml`
- `find ... -name '*.rs'`
- `rg ...`
- `npx gitnexus analyze` in `/Users/ryanliu/Documents/uclaw`

## Executive Judgment

`jcode` is an excellent Rust coding-agent runtime reference. Its most valuable ideas are compile-time module boundaries, tool/provider core traits, server-owned session state, soft interrupts, prompt-cache-aware streaming, and benchmark discipline.

`uClaw` should not become a literal jcode clone. uClaw's durable identity is a Tauri desktop Agent OS: local-first, observable, recoverable, learnable, extensible to browser, automation, IM, memory, teams, and future clusters. Its Agent OS v2 architecture is broader than jcode's coding-agent daemon architecture.

The correct migration target is:

> 100% granular absorption of jcode's backend design lessons where they strengthen uClaw's Agent OS, not 100% code-level replacement of uClaw's backend control plane.

## Framework Summary

### jcode

jcode is a modular-monolith CLI/TUI coding-agent system with a daemon/server center.

Key framework properties:

- One main binary with multiple roles: CLI command, TUI client, `serve` daemon, debug/control surfaces.
- A Tokio multi-thread runtime built explicitly in `src/main.rs`.
- A local Unix socket server model under `src/server/`, with daemon lock/readiness handling in `src/server/socket.rs`.
- Root crate still owns most product orchestration, but stable contracts are split into many crates:
  - `jcode-agent-runtime`: interrupt, graceful shutdown, stream errors.
  - `jcode-provider-core`: provider trait, split prompt, model catalog, pricing, failover.
  - `jcode-tool-core`: `Tool`, `ToolContext`, stdin request, execution mode.
  - `jcode-message-types`, `jcode-session-types`, `jcode-protocol`, `jcode-task-types`: protocol and DTO boundaries.
- Agent loop is turn/stream centric: `Agent` holds provider, registry, session, tool locks, provider session id, soft interrupt queue, background signal, cache tracker, and memory state.
- Safety is comparatively simple: tiered auto-allow vs permission queue, with action transcript.

### uClaw

uClaw is a Tauri v2 desktop Agent OS backend, currently implemented as a large `uclaw_core` crate plus utility crates.

Key framework properties:

- Tauri desktop shell plus local Axum HTTP/WebSocket server on `127.0.0.1:27270`.
- `AppState` is the central DI container for DB, sessions, providers, safety, memory, MCP, browser, automation, proactive, harness, registry hub, gbrain, memU, files rail, and more.
- `main.rs` owns observability, unclean shutdown recovery, app setup, tray, HTTP server, service boot, capability registry seeding, and many event consumers.
- Agent loop is delegate/state-machine centric: `run_agentic_loop(delegate, ReasoningContext, AgenticLoopConfig)`.
- Strategic contracts exist in `runtime/contracts.rs`: `IntentSpec`, `TaskSpec`, `TaskEvent`, autonomy ladder, policy, budgets, checkpoints.
- Capability Mesh and World Projection exist as emerging code surfaces under `registries/` and `world/`.
- Safety is more product-grade than jcode: SafetyMode, path policy, permission rules, audit log, pending approvals, Plan mode.
- Memory is broader and more complex: legacy `memory.rs`, frozen `memory_graph`, memU subprocess, gbrain Bun bridge, learning/facet cache, browser memory adapter.

## Granular Difference Matrix

| Dimension | jcode | uClaw | uClaw Upgrade Point | Priority |
|---|---|---|---|---|
| Runtime identity | Coding-agent CLI/TUI daemon | Desktop Agent OS shell | Keep uClaw identity; borrow daemon control/debug ideas only | P0 |
| Boot model | Thin `main.rs`, explicit Tokio runtime, server readiness fd/socket | Tauri setup plus many boot responsibilities in `main.rs` | Move boot composition to a boot module and time each stage | P1 |
| Process/client model | Single server owns sessions; many clients attach | Tauri app owns runtime; local API is side channel | Add session attach/replay/debug control, do not replace Tauri | P2 |
| State container | Server runtime structs + per-session agents | Huge `AppState` DI container | Split `AppState` into domain state structs behind same Tauri state | P1 |
| Agent loop | `Agent::run_turn*` stream-centered loop | `run_agentic_loop` delegate/stage-centered loop | Keep uClaw loop; import soft interrupt/background safe points | P0 |
| Interrupt model | `InterruptSignal`, soft interrupt queue, background tool signal | CancellationToken, heartbeat, stop session, recovery | Add non-destructive soft interrupt to uClaw task runtime | P1 |
| Provider abstraction | Wide trait in `jcode-provider-core`, split prompt, routes, failover | Thin lower `LlmProvider` plus stateful `ProviderService` | Extract `uclaw-provider-core` with split prompt/cost/failover traits | P1 |
| Tool abstraction | `ToolContext` carries session/message/tool/cwd/stdin/shutdown/mode | `execute(params)` plus approval/path/preview hooks | Add `ToolContext` without losing uClaw approval/path policy | P1 |
| Tool registry | Base tool cache, per-session registry clone, deterministic definitions | Simple boxed tool registry, plus separate Capability Mesh pilot | Merge runtime registry with Capability Mesh descriptors | P1 |
| Safety | Queue/history/transcript, auto-allowed tiers | DB-backed permission rules, modes, path policy, Plan mode | Keep uClaw safety; add transcript and TaskEvent audit receipts | P1 |
| Memory | JSON/graph/embedding/sidecar within coding-agent model | gbrain primary, memU, frozen memory_graph, learning, profile facets | Do not copy jcode memory writes; use gbrain receipts/context tools | P0 |
| Browser | Tool/bridge oriented | BrowserContextManager, BrowserAgentLoop, checkpoints, auth | uClaw is stronger; consolidate legacy/new browser control planes | P1 |
| Automation | Ambient/schedule commands | Humane Automation runtime, IM channels, live-room automation | Keep uClaw automation; normalize to IntentSpec/TaskEvent | P1 |
| Multi-agent | Server swarm plan/status/channel model is mature | Teams/workers/Symphony/automation are parallel surfaces | Borrow server-owned plan state; map into Agent OS workers | P1 |
| Observability | Runtime memory logs, startup/tool/session benchmarks | tracing, metrics, token budget, crash recovery, harness | Add performance scorecards and turn-level event taxonomy | P1 |
| Persistence | JSON session snapshot + journal + startup stub | SQLite migrations, rollout JSONL, per-feature DBs | Add journal/checkpoint as projection layer, not DB replacement | P1 |
| Extensibility | Tool/provider crates; plugin layer weaker | ADR defines PluginRegistry/HookBus/CapabilityProfile | uClaw target is stronger; wire pilots into main path | P0 |

## What uClaw Should Preserve

Do not replace these with jcode equivalents:

- Tauri desktop shell and local API topology.
- `runtime/contracts.rs` Agent OS v2 contracts.
- `SafetyManager`, `permissions.rs`, path policy, and Plan mode.
- gbrain-primary memory strategy.
- `memory_graph` freeze.
- Browser Agent v2 direction.
- Harness/self-improvement gates.
- World Projection and Capability Mesh north star.
- React UI/IPC product shell.
- SQLite migration history and local-first DB model.

## What uClaw Should Import

High-confidence imports:

- `ToolContext` design.
- Provider `complete_split` design and provider route/failover metadata.
- Soft interrupts and background-tool safe points.
- Tool definition ordering and base tool cache.
- Session journal/startup stub pattern for task projection.
- Runtime memory and startup benchmarks.
- Stronger protocol/type crates.
- Server-owned plan/swarm state ideas, translated into Worker/Team runtime.

## Main Risk

The biggest risk is creating a second control plane.

If uClaw copies jcode server/session/swarm/memory semantics wholesale, it will have two competing truths:

- jcode-style daemon/session state.
- uClaw Agent OS `IntentSpec` / `TaskSpec` / `TaskEvent` / World Projection state.

That would make debugging worse and violate the current ADR direction. Every imported jcode pattern must be mapped to an existing or intended uClaw Agent OS contract.

## Recommended Framing

Use this framing for follow-up implementation:

- "jcode parity" means parity in backend engineering quality.
- It does not mean cloning jcode's user interface, daemon topology, or memory authority.
- The target is uClaw Agent OS v2 with jcode-grade Rust modularity, provider/tool discipline, streaming resilience, and performance harnesses.

## ADR Alignment Review Addendum

Second-pass review against `/Users/ryanliu/Documents/uclaw/docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` found four gaps in the first comparison package:

| Gap | Prior Bias | ADR-Corrected Direction |
|---|---|---|
| Runtime kernel | Focused heavily on jcode's agent/server/session mechanics | Keep uClaw's small Agent OS runtime kernel as the composition root; import jcode mechanics only as kernel primitives |
| Teams/subagents | Treated jcode swarm as a module to borrow | Map jcode swarm to ADR `WorkerRole`, `TeamSpec`, `TeamChannel`, `ReviewGate`, and WorkerRegistry concepts |
| Browser | Described uClaw browser as stronger but did not explain why | uClaw Browser Agent v2 is closer to ADR §12 because it has per-session profiles, checkpoints, task runs, auth-profile broker, intervention bridge, memory adapter, and harness cases |
| Ambient/harness | Underweighted jcode ambient/harness | Ambient maps to uClaw Automation/ScheduledWorker, not a new loop; jcode harness maps to lightweight smoke/perf campaigns, while uClaw harness remains the canonical Evolution Layer gate |

Corrected framework rule:

> jcode is a strong runtime-component reference. The ADR is the operating-system boundary. Every jcode import must become an IntentSpec, TaskSpec, TaskEvent, Capability Card, WorldProjection update, policy hook, or harness case.

### Seven-Layer Mapping

| ADR Layer | jcode Reference | uClaw Target |
|---|---|---|
| Intent Layer | `goal`, `todo`, `swarm propose_plan`, ambient schedule context | Typed `IntentSpec` for chat, automation, team, ambient wakeups, and browser tasks |
| Runtime Kernel | `Agent::run_turn*`, soft interrupts, background tools | Existing `run_agentic_loop` plus soft interrupt/background safe points |
| Context Fabric | `read`, `agentgrep`, `session_search`, `conversation_search`, swarm shared context | `context.search/read/fold/cite` tools with citations and budgets |
| Capability Mesh | `Registry`, `ToolContext`, provider/tool crates | Tool/provider/plugin/worker capability cards and profiles |
| World Projection | server events, TUI status, side panel snapshots | `TaskEvent -> WorldProjection -> UI surfaces` |
| Safety & Policy | safety queue, ambient `request_permission` | uClaw SafetyManager/path policy plus hook-visible boundary events |
| Evolution Layer | selfdev, harness scripts, swarm benchmarks | uClaw harness episodes, scorecards, promotion gate, rollback |

This addendum narrows the earlier "replicate jcode" language: replicate the component design quality, not the top-level runtime identity.

# 02. Workspace And Module Structure Comparison

Status: analysis document, no implementation changes.
Date: 2026-05-23
Scope: Cargo workspace, crate boundaries, module ownership, and rationalized structure plan.

## Evidence

Primary files inspected:

- `/Users/ryanliu/Documents/jcode/Cargo.toml`
- `/Users/ryanliu/Documents/jcode/docs/MODULAR_ARCHITECTURE_RFC.md`
- `/Users/ryanliu/Documents/jcode/docs/CRATE_OWNERSHIP_BOUNDARIES.md`
- `/Users/ryanliu/Documents/jcode/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/src/agent.rs`
- `/Users/ryanliu/Documents/jcode/src/server.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/mod.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-provider-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-tool-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-protocol/src/lib.rs`
- `/Users/ryanliu/Documents/uclaw/Cargo.toml`
- `/Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/lib.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/main.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/dispatcher.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/contracts.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/registries/mod.rs`

Workspace counts from local scan:

- jcode: 50 crates under `/Users/ryanliu/Documents/jcode/crates`.
- uClaw: 15 crates under `/Users/ryanliu/Documents/uclaw/crates`, mostly `uclaw-utils-*`.
- uClaw core product backend remains centered in `/Users/ryanliu/Documents/uclaw/src-tauri/src`.

## Topology Comparison

| Area | jcode | uClaw | Assessment |
|---|---|---|---|
| Workspace shape | Many small crates plus large root runtime crate | One product crate plus utility crates | jcode has better compile-time boundaries |
| Product host | Root `jcode` crate | `src-tauri` crate exposing `uclaw_core` and `uclaw` bin | uClaw host is broader and more coupled |
| Type boundaries | `jcode-message-types`, `jcode-session-types`, `jcode-task-types`, `jcode-protocol` | `agent/types.rs`, `ipc.rs`, `runtime/contracts.rs`, DB DTOs, UI TS types | uClaw should extract stable DTO crates |
| Tool boundary | `jcode-tool-core` trait/context plus root implementations | `agent/tools/tool.rs` plus builtins/MCP/memU tools | uClaw should import ToolContext boundary |
| Provider boundary | `jcode-provider-core` trait/model/cost/failover | `llm/` provider trait plus `providers/` config service | uClaw needs a real provider core crate |
| Runtime contracts | Implied by server/session/protocol crates | Explicit Agent OS v2 `IntentSpec`, `TaskSpec`, `TaskEvent` | uClaw target architecture is stronger |
| UI boundary | TUI crates and protocol wire | React/Tauri IPC and local API | Product shells are not interchangeable |

## jcode Structural Pattern

jcode's structure is best understood as four rings:

1. Pure type crates:
   - `jcode-message-types`
   - `jcode-session-types`
   - `jcode-task-types`
   - `jcode-tool-types`
   - `jcode-auth-types`
   - `jcode-usage-types`

2. Core behavior contracts:
   - `jcode-provider-core`
   - `jcode-tool-core`
   - `jcode-agent-runtime`
   - `jcode-storage`
   - `jcode-protocol`

3. Product runtime:
   - root `jcode` crate: CLI, server, agent orchestration, tools, sessions, auth, memory, browser, safety.

4. Presentation/client crates:
   - `jcode-tui-*`
   - `jcode-desktop`
   - `jcode-mobile-*`

This is not a perfectly pure microservice architecture. The root `jcode` crate is still large. The advantage is that core traits and DTOs can be reused and tested independently.

## uClaw Structural Pattern

uClaw currently has three rings:

1. Utility crates:
   - `uclaw-utils-home`
   - `uclaw-utils-cache`
   - `uclaw-utils-file-watcher`
   - `uclaw-utils-path-utils`
   - other small utility crates.

2. Product backend monolith:
   - `src-tauri/src/agent`
   - `browser`
   - `automation`
   - `providers`
   - `llm`
   - `mcp`
   - `gbrain`
   - `memu`
   - `memory_graph`
   - `harness`
   - `proactive`
   - `runtime`
   - `registries`
   - `world`
   - `tauri_commands.rs`

3. Product shell:
   - Tauri main binary.
   - React UI in `/Users/ryanliu/Documents/uclaw/ui`.
   - Local HTTP/WebSocket API.

uClaw module names are already good; the issue is boundary enforcement. Many modules share `AppState`, SQLite connection, `AppHandle`, and cross-module types directly.

## Key Module Boundary Comparison

| Module | jcode Boundary | uClaw Boundary | Recommended Action |
|---|---|---|---|
| Message/types | Crate-level DTOs | Mixed Rust modules and TS bridge types | Extract `uclaw-message-types` or `uclaw-agent-types` |
| Tool trait | `ToolContext` includes session/message/tool/cwd/stdin/shutdown/mode | `execute(params)` receives no rich context | Add `ToolContext`, then move trait to `uclaw-tool-core` |
| Provider trait | `complete`, `complete_split`, model routes, failover, pricing | Split between `llm` and `providers` | Create `uclaw-provider-core`; keep `ProviderService` as configuration/control |
| Session state | Session DTO and server-owned session agents | SQLite + in-memory `SessionManager`; task contracts not fully driving runtime | Add task/session journal and projection store |
| Runtime | Agent runtime primitives extracted minimally | `runtime/contracts.rs` has stronger Agent OS spec | Preserve contracts; make main agent path consume them |
| Safety | Queue/transcript plus action tiers | Modes, path policy, DB rules, audit | Keep uClaw safety; add transcript/TaskEvent integration |
| Browser | Browser as tool/client capability | Browser v2 as runtime with context/checkpoints | uClaw stronger; consolidate legacy and v2 |
| Memory | Project/global JSON graph and sidecar | gbrain/memU/memory_graph/learning | Keep gbrain as authority; do not import jcode writes |
| Multi-agent | Swarm server state and channel/plan support | Teams/workers/Symphony/automation parallel surfaces | Borrow plan/member lifecycle into Agent OS worker model |

## Proposed uClaw Crate Layout

Do not extract everything at once. The safe order is stable types first, behavior later.

### Phase A: Pure types and contracts

New crates:

- `crates/uclaw-message-types`
- `crates/uclaw-tool-types`
- `crates/uclaw-protocol-types`
- `crates/uclaw-runtime-types`

Move only serde DTOs and small pure helpers. No Tauri, rusqlite, AppHandle, subprocess, or network dependencies.

Benefits:

- Lower risk.
- Better frontend/Rust schema alignment.
- Can be tested without app boot.

### Phase B: Core traits

New crates:

- `crates/uclaw-tool-core`
- `crates/uclaw-provider-core`
- `crates/uclaw-agent-runtime`
- `crates/uclaw-worker-core`
- `crates/uclaw-browser-provider-core`
- `crates/uclaw-harness-core`

Responsibilities:

- `uclaw-tool-core`: `Tool`, `ToolContext`, `ToolOutput`, `ApprovalRequirement`, tool definition sorting.
- `uclaw-provider-core`: provider stream event, split prompt request shape, model/cost/failover metadata.
- `uclaw-agent-runtime`: soft interrupt, background tool signal, task cancellation primitives, stream supervisor traits.
- `uclaw-worker-core`: `WorkerSpec`, `WorkerRole`, worker lifecycle events, team assignment contracts.
- `uclaw-browser-provider-core`: provider-neutral browser actions, observations, readiness probes, boundary events, checkpoints.
- `uclaw-harness-core`: harness case/episode/trace/grader DTOs and adapters independent of Tauri.

### Phase C: Bridges and services

Candidates:

- `crates/uclaw-mcp-core`
- `crates/uclaw-gbrain-bridge`
- `crates/uclaw-memu-bridge`
- `crates/uclaw-local-api`
- `crates/uclaw-harness-core`

Only extract after types and traits stabilize. These touch subprocesses, DB, AppState, and Tauri events, so they are higher risk.

## Rationalized Dependency Direction

Target dependency direction:

```text
types -> core traits -> runtime orchestration -> app adapters -> Tauri/UI
```

Forbidden direction:

```text
utility/type/core crates -> AppState/Tauri/rusqlite/memU/gbrain concrete state
```

Practical rules:

- `uclaw-provider-core` must not know about Settings UI or Keychain.
- `uclaw-tool-core` must not know about Tauri `AppHandle`.
- `uclaw-runtime-types` must not know about specific providers or tools.
- `src-tauri` remains the composition root.
- DMZ files (`tauri_commands.rs`, `main.rs`, `app.rs`, `db/migrations.rs`) should shrink, not become migration staging areas.

## 100% jcode Replication Feasibility

Structural 100% replication is not recommended.

Why:

- jcode's structure optimizes CLI/TUI coding-agent daemon work.
- uClaw optimizes a desktop Agent OS with browser, automation, IM, memory, gbrain, memU, harness, and visual state.
- Literal replication would replace uClaw's target architecture with a narrower product architecture.

What is feasible:

- 100% granular mapping of jcode module responsibilities to uClaw responsibilities.
- 1:1 replication of selected stable boundaries:
  - provider core trait and split prompt pattern.
  - tool core trait and ToolContext.
  - runtime interrupt primitives.
  - protocol/type crate separation.
  - session journal/checkpoint pattern.
  - server-owned plan/member lifecycle concepts.

## Migration Priority

| Priority | Work | Reason |
|---|---|---|
| P0 | Keep Agent OS contracts as the target | Prevent jcode from becoming a second truth source |
| P1 | Extract DTO/type crates | Lowest-risk boundary win |
| P1 | Add ToolContext | Directly improves tool design and future capability mesh |
| P1 | Extract provider core | Needed before multi-provider/failover expands further |
| P1 | Connect TaskEvent to main agent path | Turns ADR into runtime fact |
| P1 | Add worker/team core | Required for ADR L5 teams; prevents jcode swarm from becoming a separate runtime |
| P1 | Add browser-provider core | Required before comparing LocalChromium, jcode bridge, browser-use, Browserbase, Firecrawl |
| P1 | Add harness core | Required for tool/browser/team/ambient parity cases and Evolution Factory gates |
| P2 | Extract subprocess bridges | Higher risk due to memU/gbrain process lifecycle |
| P2 | Add daemon/debug attach model | Useful, but not core to product identity |

## ADR Gap Corrections

The first-pass structure plan underweighted four ADR objects:

1. `WorkerRegistry`: jcode has mature swarm/session coordination, but uClaw must represent this as workers and teams, not as copied daemon state.
2. `BrowserProvider`: jcode browser is a tool/provider bridge; uClaw's browser should become a provider family with local/external implementations.
3. `HarnessSubject`: uClaw already has a stronger generic harness core; it should be extracted before tool/browser/team reconstruction so every slice gets an evaluation path.
4. `Automation/ScheduledWorker`: jcode ambient is conceptually useful, but its storage/loop should map into uClaw automation sources and heartbeat/thread wakeups.

Additional crate candidates after Phase B stabilizes:

| Candidate Crate | Why | Avoid |
|---|---|---|
| `uclaw-worker-core` | Makes subagents/team/cluster workers one type family | embedding current `agent/teams/orchestrator.rs` loop internals |
| `uclaw-team-types` | `TeamSpec`, `TeamChannelMessage`, `ReviewGate`, role output contracts | treating team messages as hidden chat |
| `uclaw-browser-provider-core` | BrowserProvider API, action/result/observation/checkpoint DTOs | copying jcode Firefox bridge as the only provider |
| `uclaw-automation-types` | ScheduledWorker and ambient-like wake requests | creating a second scheduler beside automation |
| `uclaw-harness-core` | Case, episode, trace, grader, artifact contracts | coupling harness DTOs to Tauri/AppState |

## Bottom Line

Use jcode as a structural discipline reference:

- type crates,
- core traits,
- runtime primitives,
- session journal,
- provider/tool isolation.

Do not use jcode as a product architecture replacement.

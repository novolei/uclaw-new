# 04. Backend Reconstruction Feasibility And Blueprint

Status: analysis document, no implementation changes.
Date: 2026-05-23
Scope: feasibility of reusing jcode to refactor/upgrade uClaw backend.

## Executive Judgment

The backend refactor is feasible if "replicate jcode" means:

- replicate module boundaries,
- replicate core trait discipline,
- replicate streaming resilience,
- replicate tool/provider/session engineering patterns,
- replicate benchmark and observability discipline.

The refactor is not advisable if "replicate jcode" means:

- replace uClaw's Tauri/AppState control plane with jcode's daemon,
- replace uClaw's Agent OS v2 contracts,
- replace gbrain-primary memory,
- replace uClaw safety/path policy,
- replace browser/automation/harness surfaces,
- make jcode's session/server/swarm model the new canonical runtime truth.

The practical target:

> uClaw Agent OS v2, reconstructed with jcode-grade Rust backend modularity.

## Evidence

Primary jcode backend evidence:

- `/Users/ryanliu/Documents/jcode/Cargo.toml`
- `/Users/ryanliu/Documents/jcode/src/main.rs`
- `/Users/ryanliu/Documents/jcode/src/cli/dispatch.rs`
- `/Users/ryanliu/Documents/jcode/src/server.rs`
- `/Users/ryanliu/Documents/jcode/src/server/socket.rs`
- `/Users/ryanliu/Documents/jcode/src/server/*`
- `/Users/ryanliu/Documents/jcode/src/agent.rs`
- `/Users/ryanliu/Documents/jcode/src/agent/turn_execution.rs`
- `/Users/ryanliu/Documents/jcode/src/agent/turn_streaming_broadcast.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/mod.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/bash.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/grep.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/glob.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/apply_patch.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/agentgrep.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/read.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/write.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/edit.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/multiedit.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/patch.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/bg.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/batch.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/task.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/communicate.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/browser.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/ambient.rs`
- `/Users/ryanliu/Documents/jcode/src/browser.rs`
- `/Users/ryanliu/Documents/jcode/src/ambient/runner.rs`
- `/Users/ryanliu/Documents/jcode/src/ambient/scheduler.rs`
- `/Users/ryanliu/Documents/jcode/src/provider/openai_stream_runtime.rs`
- `/Users/ryanliu/Documents/jcode/src/safety.rs`
- `/Users/ryanliu/Documents/jcode/src/session/persistence.rs`
- `/Users/ryanliu/Documents/jcode/src/bin/harness.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-agent-runtime/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-tool-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-provider-core/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-protocol/src/lib.rs`

Primary uClaw backend evidence:

- `/Users/ryanliu/Documents/uclaw/BEHAVIOR.md`
- `/Users/ryanliu/Documents/uclaw/CONTEXT.md`
- `/Users/ryanliu/Documents/uclaw/docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/main.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/app.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/agentic_loop.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/dispatcher.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/tool.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/builtin/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/llm/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/providers/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/mcp.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/memu/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/gbrain/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/automation/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/teams/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/workers/spec.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/safety/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/registries/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/world/*`

## 1:1 Replication Matrix

| jcode Module / Responsibility | uClaw Current Module | Action | Risk | Verification |
|---|---|---|---|---|
| `jcode-message-types` | `agent/types.rs`, `ipc.rs`, UI TS types | Extract stable message DTO crate | Medium | `cargo check -p uclaw-message-types` |
| `jcode-tool-types` | `ToolOutput`, tool result payloads | Extract tool DTO crate | Medium | `cargo test -p uclaw-tool-types` |
| `jcode-protocol` | Tauri events, local API, `runtime/contracts.rs` | Create uClaw protocol types mapped to TaskEvent | High | reducer/schema tests |
| `jcode-tool-core::ToolContext` | `agent/tools/tool.rs` lacks context | Add `ToolContext` and adapter | High | `cargo test -p uclaw --lib agent::tools` |
| Base tool cache and sorted tool definitions | `ToolRegistry::list_definitions` sorts, but no base cache | Add base cache/session clone semantics | Medium | tool registry tests |
| `Agent::run_turn*` | `run_agentic_loop` delegate loop | Keep uClaw loop; import safe-point patterns | High | `cargo test -p uclaw --lib agent::agentic_loop` |
| `InterruptSignal`, soft queue | CancellationToken/stop/heartbeat | Add soft interrupt/background-tool signals | Medium | runtime cancellation tests |
| jcode bash progress/checkpoint | uClaw shell timeout/output cap/approval | Add progress/checkpoint protocol to shell | Medium | shell tool tests |
| `apply_patch` tool | uClaw edit/write preview | Add patch semantics behind uClaw approval/path policy | High | tempdir edit/preview tests |
| `jcode-provider-core::Provider` | `llm::LlmProvider`, `providers::ProviderService` | Extract `uclaw-provider-core`; keep service config | High | provider tests |
| `complete_split` | Anthropic cache controls, byte-stable prompt | Add provider-agnostic split prompt interface | High | cache token tests |
| model route/failover/pricing | ProviderService + registry | Add capability metadata, not direct cloud auth copy | Medium | providers service tests |
| jcode session snapshot/journal | SQLite messages, rollout JSONL | Add projection journal; do not replace DB | High | session recovery tests |
| jcode server socket/control | Tauri app + local API | Borrow debug/control attach only | Medium | API/local_api tests |
| jcode safety queue/transcript | SafetyManager DB/path policy | Keep uClaw safety; add transcript/TaskEvent receipts | High | safety permission tests |
| jcode memory | memory.rs/memU/gbrain/frozen memory_graph | Do not replace; only copy prompt/display ideas | Critical | freeze scans and gbrain tests |
| jcode swarm | teams/workers/Symphony/automation | Map lifecycle to WorkerRole/TeamRuntime | High | workers/team tests |
| jcode `subagent` | `agent/teams`, `workers/spec.rs` | Rebuild as WorkerSpec/WorkerRole child TaskSpec, not ad hoc nested agent | High | worker lifecycle tests |
| jcode `batch` | no generic tool batch equivalent | Add bounded parallel subcall executor with TaskEvent sub-events | Medium/High | batch tool fixture tests |
| jcode `bg` + bash progress markers | shell daemon/background support | Add background process registry, progress/checkpoint parsing, and wait/cancel APIs | High | shell/background tests |
| jcode `agentgrep` | `grep`/`glob`, GitNexus, file tools | Import multi-mode search ideas behind Context Fabric, not as a second code index | Medium | context search tests |
| jcode `session_search` / `conversation_search` | SQLite session/message search, FTS | Add context search facade over sessions and traces | Medium | FTS/context tests |
| jcode `goal` / `todo` | Plan mode, `plan_write`, `plan_update` | Map to IntentSpec/TaskSpec/plan projection; avoid duplicate todo truth | Medium | plan projection tests |
| jcode `browser` readiness/provider bridge | Browser Agent v2 | Borrow status/setup/probe contract; keep uClaw BrowserProvider runtime | Medium | browser provider tests |
| jcode ambient schedule/permission tools | automation/proactive/thread heartbeat | Translate to ScheduledWorker + boundary policy; do not add a second scheduler | High | automation runtime tests |
| jcode CLI tool harness | uClaw harness runtime | Add model-free tool smoke cases and perf campaigns | Low/Medium | harness tool cases |
| jcode telemetry/bench scripts | tracing/metrics/harness | Add perf harness and turn telemetry taxonomy | Medium | harness perf tests |
| jcode allocator diagnostics | uClaw no allocator feature by default | Benchmark before optional feature | Low/Medium | RSS scorecard |

## Tool Migration Addendum

The jcode tool surface is worth mining, but not every tool should be copied.

| jcode Tool | Recommendation | uClaw Target | Reason |
|---|---|---|---|
| `read`, `write`, `edit`, `multiedit`, `patch`, `apply_patch` | Reimplement patterns, do not blindly copy | File/edit tools + SafetyManager + preview/diff UI | uClaw must preserve path policy and approval receipts |
| `grep`, `glob` | Strong candidate for direct design replication | Search tools under Context Fabric | `ignore::WalkBuilder`, caps, gitignore behavior, and result ordering are practical wins |
| `agentgrep` | Partial import | `context.search` / GitNexus-assisted code context | Useful modes: grep/find/outline/smart; avoid creating a competing index |
| `bash` + `bg` | Strong candidate for behavior replication | Shell/background process registry | Progress/checkpoint markers and wait-on-progress are big wins for long tasks |
| `batch` | Import bounded parallelism | Generic subcall executor | Useful for independent read/search/tool fanout; must emit sub-events and honor capability profiles |
| `subagent` | Import isolation semantics | WorkerSpec/TaskSpec child worker | Child sessions should be workers with policy, budget, and trace |
| `swarm` | Import lifecycle concepts | TeamRuntime/WorkerRegistry | Plan status, member status, reports, assignment, cleanup are useful; daemon state is not |
| `session_search`, `conversation_search` | Import as Context Fabric tools | Search prior sessions/traces with citations | Fits ADR context-as-a-tool |
| `goal`, `todo` | Translate, do not copy | Intent/plan/task projection | Avoid duplicate plan/todo truth |
| `memory` | Do not copy storage model | gbrain receipts/context tools | `memory_graph` is frozen; gbrain is canonical |
| `ambient` | Translate, do not copy | Automation/ScheduledWorker + heartbeat | jcode global ambient loop would become a second scheduler |
| `browser` | Borrow provider readiness contract | BrowserProvider readiness/status/setup probes | uClaw Browser Agent v2 is richer |
| `gmail`, `webfetch`, `websearch`, `open`, `side_panel`, `selfdev` | Case-by-case plugin/capability cards | Capability Mesh | These are edge capabilities, not kernel primitives |

Every imported tool idea should get:

- a Capability Card,
- policy hook coverage,
- TaskEvent call/result/progress events,
- output budget metadata,
- harness cases,
- UI display descriptor.

## Modules To Preserve

These uClaw modules should not be replaced by jcode code or semantics:

- `runtime/contracts.rs`: Agent OS v2 contracts are the target.
- `safety/mod.rs`, `safety/permissions.rs`, `safety/path_policy.rs`: stronger than jcode's safety tier for uClaw's product risk.
- `gbrain/`: canonical durable knowledge path.
- `memu/`: Python memory bridge, if still needed as auxiliary memory.
- `memory_graph/`: frozen legacy surface; no new writes.
- `browser/`: Browser Agent v2 is more advanced than jcode browser helper model.
- `automation/`: uClaw automation is a product domain, not a jcode ambient clone.
- `harness/`: keep promotion/evaluation discipline.
- `registries/`: Capability Mesh direction.
- `world/`: World Projection direction.
- `tauri_commands.rs`: should shrink into delegates, not be bypassed.
- SQLite migrations: must preserve historical migration chain.

## Phased Migration Blueprint

### PR-0: Legal, provenance, and boundary audit

Purpose:

- Decide which jcode ideas are reimplemented vs derived.
- If any code is copied/adapted from `openai/codex`-derived files, add required SPDX header, attribution, and NOTICE entry.
- Add a comparison-to-implementation checklist.

Allowed files:

- docs only.
- `NOTICE` only if a derived code decision is made.

Verification:

```bash
git diff --check
./scripts/install-git-hooks.sh
```

Risk:

- High if code is copied without attribution.
- Low if this remains planning-only.

### PR-1: Type crates

Purpose:

- Extract pure DTOs first.
- No behavior changes.

Candidate crates:

- `uclaw-message-types`
- `uclaw-tool-types`
- `uclaw-protocol-types`
- `uclaw-runtime-types`

Rules:

- No Tauri.
- No rusqlite.
- No AppState.
- No subprocess.
- No provider HTTP clients.

Verification:

```bash
cargo check -p uclaw-message-types -p uclaw-tool-types -p uclaw-protocol-types -p uclaw-runtime-types
cargo test -p uclaw --lib agent::types runtime::contracts
```

Risk:

- Medium: type movement can break serialization expectations.

### PR-2: ToolContext and tool-core

Purpose:

- Introduce jcode-style execution context while preserving uClaw safety.

`ToolContext` should include:

- session id,
- message id,
- tool call id,
- workspace/cwd,
- cancellation/soft interrupt handle,
- stdin/request-user bridge,
- effective safety mode,
- capability profile id,
- TaskEvent emitter,
- preview target helper,
- path resolver using uClaw path policy.

Verification:

```bash
cargo test -p uclaw --lib agent::tools safety::permissions
```

Risk:

- High: all built-in tools and MCP proxies are affected.

Mitigation:

- Add a compatibility adapter so tools can migrate one by one.
- First PR compiles without behavior changes.

### PR-3: Provider core and split prompt

Purpose:

- Add provider-agnostic split prompt and provider route/cost/failover metadata.

Keep:

- `ProviderService` as config/control plane.
- Existing credentials and Keychain/secret handling.
- CSP/provider registry rules.

Add:

- `ProviderRequest`,
- `ProviderStream`,
- `ProviderCapabilities`,
- `ProviderRoute`,
- `ProviderCostModel`,
- `complete_split`.

Verification:

```bash
cargo test -p uclaw --lib llm providers agent::telemetry
```

Risk:

- High: Anthropic/OpenAI payload shape and cache token attribution are brittle.

### PR-4: Soft interrupt and background tool protocol

Purpose:

- Add jcode-grade non-destructive interruption to uClaw.

Map to uClaw:

- soft user message -> `TaskEvent::input.injected`,
- background current tool -> `TaskEvent::tool.backgrounded`,
- graceful shutdown -> checkpoint and partial assistant recovery,
- stdin request -> existing ask_user/pending request surface.

Verification:

```bash
cargo test -p uclaw --lib runtime::task agent::agentic_loop agent::tools::builtin::shell
```

Risk:

- Medium/High: interruption semantics must not corrupt persisted conversation state.

### PR-5: Patch/edit reconstruction

Purpose:

- Add robust patch semantics while preserving uClaw path policy and preview UX.

Rules:

- Never bypass `SafetyManager`.
- Every write must have preview target metadata.
- Agent-visible errors must be structured by category.
- UI receives file touch/diff events.

Verification:

```bash
cargo test -p uclaw --lib agent::tools::builtin::edit preview safety::path_policy
```

Risk:

- High: write tools are user-trust critical.

### PR-6: Stream supervisor and telemetry

Purpose:

- Normalize streaming lifecycle across providers.

Events:

- connect start/end,
- first token,
- stall,
- retry,
- provider fallback,
- complete,
- usage/cost/cache,
- abort/cancel.

Verification:

```bash
cargo test -p uclaw --lib agent::llm_stream llm::providers observability
```

Risk:

- Medium: wrapper may fight provider-specific retry code.

### PR-7: Task/session projection journal

Purpose:

- Borrow jcode snapshot/journal pattern for Agent OS projection.

Do:

- append TaskEvent JSONL,
- create compact startup stub,
- support corruption recovery,
- keep SQLite as canonical historical DB.

Do not:

- replace agent_messages or session tables.

Verification:

```bash
cargo test -p uclaw --lib runtime::rollout agent::session harness::trace
```

Risk:

- High: projection and DB can diverge unless ownership is explicit.

### PR-8: Capability Mesh integration

Purpose:

- Make existing `RegistryHub` and resolver participate in real agent dispatch.

Steps:

- Map tools/providers/MCP/browser/automation to capability entries.
- Add reliability/cost/permission metadata.
- Let planner/delegate request capability by `CapabilityQuery`.

Verification:

```bash
cargo test -p uclaw --lib registries runtime::contracts agent::dispatcher
```

Risk:

- High: routing changes affect many workflows.

### PR-9: Worker/team runtime alignment

Purpose:

- Translate jcode swarm concepts into uClaw WorkerRole/TeamRuntime.

Borrow:

- server-owned plan,
- member status,
- file touch reports,
- blocked reason,
- completion report,
- channel/DM style coordination.

Do not borrow:

- TUI swarm UI,
- jcode server as canonical state.

Verification:

```bash
cargo test -p uclaw --lib workers agent::teams symphony_graph
```

Risk:

- High: uClaw already has teams/workers/Symphony/automation surfaces that must be reconciled.

### PR-10: Tool family reconstruction

Purpose:

- Bring jcode-grade tool behavior into uClaw without bypassing safety.

Order:

1. search/glob/agentgrep-style context search,
2. shell/background progress,
3. read/write/edit/patch preview normalization,
4. batch bounded parallel subcalls,
5. session/conversation search as Context Fabric tools.

Verification:

```bash
cargo test -p uclaw --lib agent::tools runtime::contracts safety::path_policy
```

Risk:

- High for write/shell tools; Medium for read/search/session-search.

### PR-11: BrowserProvider alignment

Purpose:

- Keep uClaw Browser Agent v2 as the superior local browser runtime.
- Borrow jcode's provider readiness, setup/status, capability probing, and metadata conventions.

Add:

- `BrowserProviderStatus`,
- `BrowserProviderCapabilities`,
- `BrowserProviderReadinessProbe`,
- provider-independent action/result/checkpoint DTOs,
- harness cases runnable against local Chromium and mock external providers.

Verification:

```bash
cargo test -p uclaw --lib browser harness::adapters::browser runtime::contracts
```

Risk:

- Medium/High: browser state, auth profiles, and checkpoints must remain session-scoped.

### PR-12: Ambient-to-automation translation

Purpose:

- Translate jcode ambient concepts into uClaw ScheduledWorker and automation runtime.

Borrow:

- adaptive wake intervals,
- user-active pause policy,
- token headroom reservation,
- direct delivery into existing sessions,
- queued directives,
- permission request context with rollback/risk fields.

Do not borrow:

- global ambient session registry,
- separate ambient state files as canonical truth,
- ambient-specific permission path.

Verification:

```bash
cargo test -p uclaw --lib automation proactive runtime::contracts safety::permissions
```

Risk:

- High: scheduled work must not starve active user tasks or create hidden side effects.

### PR-13: Harness hardening

Purpose:

- Keep uClaw's harness as the canonical Evolution Layer gate.
- Add jcode-style lightweight smoke/perf campaigns.

Add:

- model-free tool harness cases,
- browser provider parity cases,
- team/subagent runtime cases,
- scheduled worker cases,
- perf artifact JSON output,
- regression pack references for promotion gates.

Verification:

```bash
cargo test -p uclaw --lib harness
```

Risk:

- Medium: harness can become ornamental if it is not wired into promotion and PR verification.

## Risk Register

| Risk | Level | Why | Mitigation |
|---|---:|---|---|
| Agent loop replacement | Critical | Would lose GEP, gbrain prompt, token telemetry, plan guard, heartbeat, recovery | Keep uClaw loop; import patterns |
| Memory model replacement | Critical | Violates gbrain-primary and memory_graph freeze | Only map jcode memory ideas to gbrain/context receipts |
| Tool trait migration | High | Touches every tool and MCP proxy | Adapter-first migration |
| Provider trait migration | High | Payload/cache/cost differences are subtle | Add tests before switching call paths |
| Session journal divergence | High | Two sources of truth possible | Define SQLite vs projection ownership |
| Tauri command churn | High | DMZ file; broad UI IPC impact | Thin delegates, one PR per surface |
| Safety bypass | High | Trust regression | Route every write/exec through existing SafetyManager |
| Team runtime fork | High | jcode swarm concepts could duplicate uClaw workers/Symphony | WorkerSpec/TaskSpec/TaskEvent only |
| Ambient scheduler fork | High | A second scheduler would fight automation/proactive/heartbeat semantics | Translate ambient into automation ScheduledWorker |
| Browser stack regression | High | jcode bridge is narrower than uClaw Browser Agent v2 | Keep uClaw BrowserProvider; borrow readiness probes only |
| Harness drift | Medium | jcode scripts are useful but not canonical | Convert scripts into uClaw HarnessCase/Episode artifacts |
| Build churn | Medium | Crate extraction can slow short-term work | Extract pure types first |
| Observability overhead | Medium | High-frequency event logging can grow memory/CPU | Bounded rings and drop counters |
| Legal/provenance | High | Derived-code rules are strict | PR-0 inventory and NOTICE discipline |

## Final Feasibility Statement

Feasible:

- modular boundary replication,
- selected 1:1 trait and primitive migration,
- phased backend hardening,
- performance and observability parity,
- tool/provider/session discipline.

Not feasible as a safe goal:

- 100% code-level backend replacement.

Recommended end state:

```text
uClaw Agent OS v2
  + jcode-style type crates
  + jcode-style tool/provider core traits
  + jcode-style stream/session/interrupt mechanics
  + uClaw-native safety, memory, browser, automation, harness, world projection
```

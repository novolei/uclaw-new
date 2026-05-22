# 06. ADR Gap Audit And Reference Addenda

Status: second-pass analysis document, no implementation changes.
Date: 2026-05-23
Scope: audit the jcode comparison package against the accepted uClaw Agent OS v2 North Star, then add subagent/team, tool, browser, ambient, and harness corrections.

## Executive Correction

The first-pass reports were directionally correct: jcode is a strong Rust runtime reference, while uClaw should preserve Agent OS v2 as its product and architecture identity.

The main gap was emphasis. The reports focused on jcode's backend engineering quality, but they did not consistently force every imported idea through the ADR's 11 design questions:

1. user intent,
2. autonomy level,
3. canonical truth source,
4. TaskEvent entries,
5. cited context,
6. capability cards,
7. policy hooks,
8. WorldProjection,
9. harness cases,
10. rollback/disable path,
11. explicit non-ownership.

Corrected rule:

> jcode is the component-design reference. The ADR is the operating-system contract.

## Evidence Read

uClaw:

- `/Users/ryanliu/Documents/uclaw/docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `/Users/ryanliu/Documents/uclaw/BEHAVIOR.md`
- `/Users/ryanliu/Documents/uclaw/CONTEXT.md`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/contracts.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/teams/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/workers/spec.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/agent/tools/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/automation/*`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/harness/*`

jcode:

- `/Users/ryanliu/Documents/jcode/src/tool/*`
- `/Users/ryanliu/Documents/jcode/src/tool/communicate.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/task.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/batch.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/browser.rs`
- `/Users/ryanliu/Documents/jcode/src/browser.rs`
- `/Users/ryanliu/Documents/jcode/src/tool/ambient.rs`
- `/Users/ryanliu/Documents/jcode/src/ambient/runner.rs`
- `/Users/ryanliu/Documents/jcode/src/ambient/scheduler.rs`
- `/Users/ryanliu/Documents/jcode/src/bin/harness.rs`
- `/Users/ryanliu/Documents/jcode/scripts/benchmark_swarm.py`
- `/Users/ryanliu/Documents/jcode/scripts/test_soft_interrupt.py`

## Gap Audit

| Area | First-Pass Gap | Corrected Design |
|---|---|---|
| ADR grounding | Reports referenced Agent OS v2 but did not always enforce ADR §18 questions | All future implementation plans must answer ADR §18 before code |
| Subagents/teams | jcode swarm was underexplored | Treat jcode swarm as design input for `WorkerRegistry`, `TeamSpec`, `TeamChannel`, `ReviewGate`, not as a copied daemon |
| Tools | Tool migration was too generic | Add per-tool import matrix and start with low-risk search/session/context tools before write/shell tools |
| Browser | Comparison said uClaw was stronger but did not justify it granularly | uClaw Browser Agent v2 is better aligned to ADR; jcode browser contributes readiness/setup/provider metadata |
| Ambient | Not covered enough | Translate ambient into `ScheduledWorker` and automation heartbeat, not a parallel global loop |
| Harness | Underweighted jcode's smoke/perf scripts | Keep uClaw harness as canonical; add jcode-style model-free tool harness and perf campaigns |
| WorldProjection | Mentioned as target, but not applied to teams/browser/ambient/harness | Add explicit projection blocks for workers, browser provider, scheduled work, and scorecards |

## Subagent And Team Runtime Hardening

### jcode reference

jcode has three relevant layers:

- `subagent` tool: creates an isolated child session, forks provider state, blocks recursive delegation tools, and can return answer/compact/full transcript.
- `batch` tool: bounded parallel tool fanout with progress events.
- `swarm` tool: server-mediated coordination with shared context, DM/channel, plan proposal/approval/rejection, spawn, assign, await, cleanup, status, report, and plan status.

### uClaw current state

uClaw already has:

- `workers/spec.rs`: `WorkerRole`, `WorkerScope`, `WorkerStatus`, `WorkerLifecycleEvent`;
- `agent/teams/orchestrator.rs`: supervisor loop and worker spawning;
- `agent/teams/channel.rs`: persisted channel messages plus frontend events;
- ADR concepts: `TeamSpec`, `Coordinator`, `WorkerRole`, `TeamChannel`, `ReviewGate`, WorkerRegistry.

### Required hardening

| Need | Source | uClaw Design |
|---|---|---|
| Child isolation | jcode `subagent` sessions | Child workers are `TaskSpec`s with parent task id and scoped capability profile |
| Bounded parallelism | jcode `batch` `MAX_PARALLEL` | Team/subcall fanout has explicit concurrency, cost, turn, and wall-clock caps |
| Recursive delegation guard | jcode blocks `subagent`, `task`, `todo` in child | Worker capability profiles deny team-control tools unless role explicitly owns coordination |
| Shared context | jcode swarm shared context | Context Fabric `context.pin/read/release` with citations |
| Plan state | jcode plan proposal/status/assignment | `TeamSpec.planRef` and task projection, not hidden channel text |
| Completion report | jcode `swarm report` | Worker emits `worker_completed` and artifact refs |
| Reviewer stop | ADR `ReviewGate` | Reviewer verdict blocks `task_finished(done)` |
| Cleanup | jcode cleanup owned workers | WorkerRegistry drains owned workers and records termination reason |

### Missing TaskEvents

Add or standardize:

- `worker_assigned`,
- `worker_started`,
- `worker_message`,
- `worker_artifact`,
- `worker_blocked`,
- `review_requested`,
- `review_verdict`,
- `worker_finished`,
- `team_finished`.

## Tool Migration Findings

### High-value imports

| jcode Tool | Why Useful | uClaw Landing Zone |
|---|---|---|
| `grep` / `glob` | gitignore-aware, capped, parallel search | `agent/tools/builtin/search.rs`, later `context.search` |
| `agentgrep` | higher-level search modes: grep/find/outline/smart | Context Fabric over files + GitNexus |
| `bash` / `bg` | background progress/checkpoint protocol | shell tool + background process registry |
| `read` | line caps, image/PDF handling ideas | read_file + artifact preview |
| `write` / `edit` / `multiedit` / `patch` / `apply_patch` | preview and patch ergonomics | edit tools behind path policy |
| `batch` | bounded parallel tool execution | generic subcall executor |
| `subagent` | isolated child work | WorkerSpec child task |
| `swarm` | plan/member/report lifecycle | TeamRuntime |
| `session_search` / `conversation_search` | prior-session retrieval | Context Fabric session search |
| `goal` / `todo` | explicit progress/checkpoint metadata | Intent/Task/Plan projection |
| `ambient` permission/schedule | structured background boundaries | Automation/ScheduledWorker |

### Avoid or pluginize

| jcode Tool | Reason |
|---|---|
| `memory` | storage semantics conflict with gbrain-primary and memory_graph freeze |
| `gmail` | should be a plugin/capability card with external-send policy hooks |
| `side_panel` | TUI/product-specific; frontend should use WorldProjection |
| `selfdev` | map ideas to Evolution Factory, do not copy hot-reload semantics into core |
| `browser` | useful readiness contract, but uClaw browser runtime is stronger |

## Browser Design Comparison

### jcode browser design

Strengths:

- single `browser` tool with provider abstraction;
- explicit `status` and `setup` actions;
- readiness probes for required bridge actions;
- Firefox native messaging bridge install/repair workflow;
- session socket and process liveness checks;
- action metadata includes backend/browser/setup status;
- provider abstraction is simple and tool-friendly.

Weaknesses for uClaw's ADR target:

- centered on a Firefox bridge rather than a provider family;
- browser state is mostly tool/action oriented;
- less complete for task-level checkpoints, auth profiles, intervention boundaries, and visual task monitoring;
- not a full BrowserProvider architecture.

### uClaw browser design

Strengths:

- per-session `BrowserContextManager`;
- per-session and identity-scoped browser profiles;
- Browser Agent task loop with observe/decide/act/recover/checkpoint;
- auth-profile broker and storage-state application;
- boundary detection and ask-user intervention bridge;
- task store, checkpoint resume, browser memory adapter;
- Browser harness cases;
- UI monitor and screencast path;
- closer to ADR §12 BrowserProvider and §14 automation/team/cluster goals.

Weaknesses:

- legacy `BrowserService` still exists as compatibility surface;
- BrowserProvider trait is not yet formalized;
- provider readiness/setup/status semantics are less clean than jcode;
- browser tool surface is large and can add token pressure unless capability profiles gate it;
- provider-independent harness needs more work.

### Verdict

uClaw's browser design is more reasonable and more advanced for Agent OS v2. jcode is better only in the narrow area of provider readiness/setup ergonomics.

Recommendation:

- keep uClaw Browser Agent v2 as `LocalChromiumProvider`;
- borrow jcode's `status/setup/ensure_ready/capability_probe` contract;
- add mock provider and external provider stubs only behind `BrowserProvider`;
- sunset legacy browser features by routing compatibility commands through provider adapters.

## Ambient Design Comparison

### jcode ambient design

Strengths:

- persistent scheduled queue;
- ambient state with last run, summary, compactions, memory modifications, total cycles;
- adaptive scheduler based on user activity, provider token headroom, and rate-limit backoff;
- direct delivery to an existing session or spawning a new session;
- soft interrupt injection from external channels into active ambient cycles;
- structured permission request with rationale, risks, planned steps, rollback, expected outcome;
- visible-cycle context and completion summary.

Weaknesses for uClaw:

- global ambient state would duplicate uClaw automation/proactive/heartbeat systems;
- ambient-specific session registry and permission path would bypass Agent OS contracts if copied literally;
- memory counters are not aligned with gbrain receipts and TaskEvent traces.

### uClaw target

Map jcode ambient concepts to:

- `IntentOrigin::Automation` or `IntentOrigin::System`;
- autonomy level `ScheduledWorker`;
- automation run ledger;
- thread heartbeat for user-visible follow-ups;
- `BoundaryYield` for permission/escalation;
- gbrain receipts for memory writes;
- `TaskEvent` stream and harness episode for trace/evidence.

Do not create a second ambient loop.

## Harness Comparison

### jcode harness design

Strengths:

- `src/bin/harness.rs` can run a model-free tool smoke test over write/read/edit/patch/ls/glob/grep/bash/todo/batch;
- many focused scripts exist for startup, compile, tool, memory, swarm, soft interrupt, reload, and e2e behavior;
- tests are cheap to invoke and good at catching local runtime regressions.

Weaknesses:

- harness is less connected to a product-wide evolution/promotion model;
- scripts are valuable but not all first-class typed episodes;
- not every benchmark has a structured artifact/rollback relationship.

### uClaw harness design

Strengths:

- generic `HarnessCase`, `HarnessEpisode`, `HarnessEvent`, `HarnessArtifact`, `HarnessGrader`;
- adapters for agent loop, browser, memory, live-room;
- `TaskEventSource` mapping;
- self-improvement gate with evidence, score, blocker, rollback policy;
- closer to ADR Evolution Factory.

Weaknesses:

- needs more model-free smoke tests;
- needs perf campaigns for tool/browser/team/scheduled worker;
- automation has a temporary source mapping caveat in current code comments;
- harness output should become more visible in WorldProjection and PR verification.

### Verdict

uClaw harness is architecturally superior. jcode should contribute runnable smoke/perf discipline.

Recommended imports:

- model-free tool harness,
- browser provider parity harness,
- team/subagent benchmark,
- soft interrupt test campaign,
- startup/visible-ready scorecard,
- JSON artifacts with p50/p95 and regression thresholds.

## Updated Implementation Gate

Before any jcode-inspired implementation PR, write a plan that includes this table:

| ADR Question | Required Answer |
|---|---|
| Intent | Which `IntentOrigin` and user goal this supports |
| Autonomy | Max autonomy rung and risk cap |
| Truth source | DB, TaskEvent stream, gbrain, harness artifact, or projection |
| TaskEvent | Events emitted and event payload refs |
| Context | What is read, from where, and how cited |
| Capability | Capability Card entries added/consumed |
| Hooks | Policy hooks that can block/mutate |
| Projection | WorldProjection block(s) |
| Harness | Cases and scorecards proving behavior |
| Rollback | Disable/rollback path |
| Non-ownership | What this does not own |

If the table is unclear, the slice is still architecture work, not implementation work.

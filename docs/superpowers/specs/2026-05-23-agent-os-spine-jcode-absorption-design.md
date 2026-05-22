# uClaw Agent OS Spine and jcode Absorption Design

Status: PR-0 design baseline
Date: 2026-05-23
Owner: Ryan Liu
Scope: project-level upgrade route for absorbing the useful backend design
patterns from `/Users/ryanliu/Documents/jcode` into uClaw without replacing
uClaw's Agent OS v2 direction.

## 1. Executive Decision

uClaw should not "copy jcode as a second runtime". uClaw should absorb jcode
as a reference implementation for concrete runtime components:

- tool context, approval, and tool harness ergonomics;
- soft interrupts, background job progress, and safe points;
- browser provider readiness and capability probing;
- subagent, batch, and team work orchestration;
- ambient scheduled work semantics;
- model-free smoke, performance, and regression campaigns.

The target architecture remains the ADR north star:

- one small Rust runtime kernel;
- one task protocol: `IntentSpec`, `TaskSpec`, `TaskEvent`;
- one Context Fabric and one Capability Mesh;
- one Safety/Policy model;
- one UI truth model: `WorldProjection`;
- one harness gate before promoting autonomy.

This spec is PR-0. It only creates a shared design baseline and PR discipline.
It does not change runtime code.

## 2. Why This Exists

The jcode comparison reports found strong reusable ideas, but they also exposed
a risk: a literal 1:1 port would bring in duplicate control-plane concepts
that conflict with uClaw's Agent OS v2 contract.

The goal is therefore precise:

1. Preserve uClaw's OS-level design.
2. Port jcode's proven component patterns into that OS.
3. Make every later PR small, reviewable, testable, and reversible.
4. Use the Superpowers workflow for every PR so the implementation stays
   deliberate rather than becoming a broad rewrite.

## 3. Source Material

Primary uClaw sources:

- `BEHAVIOR.md`
- `CONTEXT.md`
- `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- `docs/jcode_comparison/README.md`
- `docs/jcode_comparison/01_framework_design.md`
- `docs/jcode_comparison/02_workspace_and_modules.md`
- `docs/jcode_comparison/03_performance_optimization.md`
- `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
- `docs/jcode_comparison/05_frontend_integration.md`
- `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`

Primary jcode reference areas:

- `/Users/ryanliu/Documents/jcode/src/tool`
- `/Users/ryanliu/Documents/jcode/src/ambient`
- `/Users/ryanliu/Documents/jcode/src/harness`
- jcode browser runtime and readiness flows
- jcode subagent, batch, background, and swarm flows

## 4. Non-Negotiable Constraints

- Do not write to `memory_graph`; durable facts go through `gbrain`.
- Do not introduce a second scheduler, browser stack, or control plane.
- Do not bypass `SafetyManager`, approval flows, or policy hooks.
- Do not add new `dirs::home_dir().*".uclaw"` call sites.
- Do not touch DMZ files without the writer/reviewer protocol.
- Do not port derived code without Apache-2.0 SPDX attribution and NOTICE
  updates where required.
- Do not start implementation without a plan in `docs/superpowers/plans/`.
- Do not edit a symbol without GitNexus impact analysis.
- Do not commit without `gitnexus detect-changes` and a verification command.

## 5. Product Definition

The desired product is a generally useful Agent OS client:

- easy enough for normal users to start useful work without learning runtime
  internals;
- powerful enough for long-running agent, browser, automation, and team work;
- stable enough that interrupted or background work can resume;
- observable enough that users can understand what happened and why;
- extensible enough that tools, browsers, models, workers, and external
  platforms can be added as capability cards;
- measurable enough that autonomy levels are promoted only through harness
  evidence.

In plain terms: uClaw should feel simple because the runtime underneath is
disciplined.

## 6. ADR Section 18 Answers

| Question | Answer |
|---|---|
| 1. What user intent does this support? | Long-running local-first work across agent chat, browser tasks, automation, background jobs, and agent teams. |
| 2. What autonomy level can it run at? | Human-driven by default; assisted background work after explicit user start; scheduled or ambient work only through automation specs, capability policy, and harness gates. |
| 3. What is the canonical truth source? | Runtime truth is `TaskEvent`; durable knowledge is `gbrain`; UI truth is `WorldProjection`; per-domain stores remain implementation details behind adapters. |
| 4. What TaskEvent entries does it emit? | `task.started`, `tool.requested`, `tool.approved`, `tool.progress`, `tool.completed`, `tool.failed`, `worker.spawned`, `worker.progress`, `browser.ready`, `browser.action`, `boundary.yielded`, `checkpoint.saved`, `projection.updated`, `harness.case.completed`. Exact variants are implementation-PR owned. |
| 5. What context does it read, and how is it cited? | Session messages, gbrain facts, tool outputs, browser run state, automation specs, worker traces, filesystem evidence, and GitNexus context. Every context read must carry a source id or citation handle into TaskEvent or the harness trace. |
| 6. What capability cards does it add or consume? | Consumes existing tools, browser, provider, memory, automation, and safety capabilities. Adds or formalizes cards for BrowserProvider, ScheduledWorker, TeamWorker, BackgroundJob, ToolHarnessSubject, and ContextSearch. |
| 7. What policy hooks can block it? | SafetyManager approvals, file/network/shell policy, browser login policy, automation autonomy policy, provider credential policy, workspace isolation policy, memory write policy, and DMZ review policy. |
| 8. What world projection does the UI render? | A unified projection of current task state: intent, active workers, tool calls, browser readiness, background jobs, checkpoints, user boundaries, approvals, costs, evidence, and harness status. |
| 9. What harness cases prove it works? | Model-free tool smoke tests, browser readiness smoke tests, background job resume tests, scheduled worker pause/resume tests, worker/team trace tests, TaskEvent projection replay tests, and full Agent/Chat/Browser/Automation scorecards. |
| 10. What is the rollback or disable path? | Each PR ships behind a small adapter, feature flag, capability card, or inactive registry entry. Rollback disables the new adapter and falls back to the existing uClaw runtime path. |
| 11. What does it deliberately not own? | It does not own memory_graph revival, a new frontend shell, a jcode TUI clone, a second browser runtime, a second automation scheduler, provider rewrites unrelated to the task protocol, or unrelated UI redesign. |

## 7. Target Architecture

The upgrade has one spine and six component families.

### 7.1 Spine

The spine is:

```text
User intent
  -> IntentSpec
  -> TaskSpec
  -> TaskEvent stream
  -> WorldProjection
  -> Harness scorecard
```

Every implementation PR should strengthen this spine. If a change cannot say
which part of the spine it touches, it is probably not ready.

### 7.2 Runtime Kernel

The runtime kernel owns lifecycle, cancellation, checkpointing, safe points,
progress, and event emission. It should stay small.

jcode influence:

- soft interrupts;
- background progress;
- safe checkpoint boundaries;
- structured task status.

uClaw decision:

- port the behavior as `TaskEvent` and boundary-yield semantics;
- avoid porting jcode daemon or TUI control-plane assumptions.

### 7.3 Capability Mesh

Tools, browser providers, workers, memory, model providers, and automations
must appear as capability cards. The mesh should answer:

- what this capability can do;
- what context it needs;
- what policy can block it;
- what events it emits;
- what harness cases prove it.

jcode influence:

- tool families under `src/tool`;
- tool setup/status commands;
- direct model-free tool harnesses.

uClaw decision:

- translate each useful jcode tool family into a uClaw capability card;
- prefer adapter reuse over copying runtime-specific command assumptions.

### 7.4 Context Fabric

Context reads should be explicit, cited, and budgeted.

jcode influence:

- context and conversation search tools;
- agentgrep-style focused retrieval;
- goal/todo/task-state helpers.

uClaw decision:

- map these into Context Fabric providers;
- use gbrain for durable knowledge;
- use GitNexus for code intelligence;
- avoid creating a parallel memory stack.

### 7.5 BrowserProvider

The browser layer should be provider-shaped rather than hardwired.

jcode influence:

- readiness checks;
- setup/status commands;
- capability probes;
- clear failure reasons before task start.

uClaw decision:

- keep uClaw Browser Agent v2 as the primary browser architecture;
- add jcode-style readiness/status/probe ergonomics to the provider boundary.

### 7.6 Worker and Team Runtime

Subagents and teams should be first-class workers, not separate mini-agents
with their own truth model.

jcode influence:

- `subagent`;
- `batch`;
- `swarm`;
- background task primitives.

uClaw decision:

- represent each child worker as `TaskSpec` plus role metadata;
- emit worker lifecycle through `TaskEvent`;
- render team state from `WorldProjection`;
- benchmark success rates before raising autonomy.

### 7.7 Scheduled and Ambient Work

Ambient behavior should be treated as automation-originated scheduled work.

jcode influence:

- scheduled queues;
- user-active pause;
- token headroom checks;
- direct delivery;
- soft interrupt and permission context.

uClaw decision:

- map ambient work into `IntentOrigin::Automation` or `IntentOrigin::System`;
- use existing automation/heartbeat concepts;
- persist receipts in gbrain where durable memory is warranted;
- never create a second ambient scheduler.

### 7.8 Harness

Harness is the promotion gate for autonomy.

jcode influence:

- model-free tool harness;
- performance campaigns;
- startup and smoke scorecards;
- interrupt and checkpoint tests.

uClaw decision:

- keep uClaw harness as the OS-level evaluator;
- add jcode-inspired subjects and scorecards;
- require scorecard evidence before capability or autonomy promotion.

## 8. Superpowers PR Workflow

Every PR in this program follows this sequence.

### 8.1 Skill Gate

At the start of each PR:

1. Use `superpowers:using-superpowers`.
2. For design/spec work, use `superpowers:brainstorming`.
3. For implementation, use `superpowers:writing-plans`.
4. For code changes, use `superpowers:test-driven-development` where practical.
5. For execution, use `superpowers:executing-plans`.
6. Before completion, use `superpowers:verification-before-completion`.
7. For major runtime changes, use `superpowers:requesting-code-review`.

### 8.2 Plan Gate

Each implementation PR needs a plan under:

```text
docs/superpowers/plans/<milestone>-<topic>.md
```

The plan must include:

- ADR Section 18 answers;
- allowed files;
- explicit non-goals;
- GitNexus impact targets;
- test-first or harness-first strategy;
- rollback path;
- expected verification output.

### 8.3 Code Intelligence Gate

Before editing any symbol:

- run GitNexus impact for that symbol;
- report direct callers, affected flows, and risk level;
- stop for user confirmation if risk is HIGH or CRITICAL.

Before committing:

- run GitNexus detect changes;
- confirm the affected flows match the plan;
- record the result in the PR notes.

### 8.4 Verification Gate

Each PR must include at least one unambiguous verification command. Examples:

```bash
cargo test -p uclaw_core -- <filter>
cd ui && npm test -- --run
cargo test -p uclaw_core harness::
git diff --check -- <changed-files>
```

The commit body or PR description must include both the command and the
expected output.

### 8.5 Review Gate

Use a fresh reviewer session for:

- DMZ file changes;
- HIGH or CRITICAL GitNexus impact;
- safety, automation, tool execution, browser login, or memory persistence
  changes;
- any PR that changes task protocol semantics.

## 9. PR Roadmap

The comparison reports currently recommend this sequence.

| PR | Theme | Purpose | Implementation Risk |
|---|---|---|---|
| PR-0 | Design baseline | This spec plus jcode comparison updates. | Low |
| PR-1 | Runtime event spine | Define or tighten task/event/projection adapter contracts without changing behavior. | Medium |
| PR-2 | ToolContext adapter | Add a jcode-inspired tool context adapter around existing uClaw tools. | Medium |
| PR-3 | Provider core | Normalize provider readiness, credentials, and streaming status. | Medium |
| PR-4 | Soft interrupts | Add safe points and resumable boundary yields. | High |
| PR-5 | Session projection journal | Make projection replay a first-class testable surface. | High |
| PR-6 | Performance scorecards | Add repeatable scorecards for latency, progress, token budget, and resume. | Medium |
| PR-7 | Subagent/team hardening | Represent workers and teams as child TaskSpecs with traceable events. | High |
| PR-8 | Tool family mesh | Translate high-value jcode tool families into uClaw capability cards. | Medium |
| PR-9 | BrowserProvider | Add status/setup/probe readiness around Browser Agent v2. | Medium |
| PR-10 | Ambient mapping | Map jcode ambient concepts onto automation/scheduled workers. | High |
| PR-11 | Harness campaigns | Add model-free tool, browser, background, and worker campaigns. | Medium |
| PR-12 | Frontend projection reducer | Render runtime truth from TaskEvent-derived projection. | High |
| PR-13 | Surface convergence | Align Agent, Chat, Browser, Automation, Symphony, and Team views on one projection. | High |

This is not a promise to land all PRs in order if the codebase reveals a better
dependency path. It is the default execution order until evidence says
otherwise.

## 10. First Implementation Slice Recommendation

After PR-0 is reviewed, the best first implementation PR is PR-1:

```text
PR-1: Runtime Event Spine Audit and Adapter Contracts
```

Why PR-1 first:

- it gives later tool/browser/team work a stable event vocabulary;
- it can be scoped narrowly;
- it reduces the risk of each later PR inventing a local truth model;
- it creates harness replay fixtures early.

PR-1 should avoid broad behavior changes. It should focus on contracts,
fixtures, and tests.

## 11. Workstream Details

### 11.1 Tools

Recommended jcode borrow:

- tool setup/status ergonomics;
- tool context object;
- background progress and cancellation;
- model-free tool harness cases;
- context search tool shape where it maps cleanly to gbrain/GitNexus.

Avoid:

- duplicating existing built-in tools without a capability-card reason;
- bypassing `SafetyManager`;
- porting command-line UX assumptions directly into Tauri IPC.

### 11.2 Browser

Recommended jcode borrow:

- readiness checks before task launch;
- provider status;
- setup diagnostics;
- capability probes;
- clear remediation text when browser runtime is unavailable.

Keep from uClaw:

- Browser Agent v2 task store and checkpoint model;
- local desktop integration;
- Tauri/WebView-facing browser surfaces;
- browser memory/checkpoint persistence.

### 11.3 Ambient and Automation

Recommended jcode borrow:

- scheduled queue semantics;
- user-active pause;
- token budget headroom checks;
- permission-context payloads;
- soft interrupt delivery.

Keep from uClaw:

- automation specs;
- heartbeat semantics;
- gbrain receipts;
- TaskEvent projection;
- user-visible approval boundaries.

### 11.4 Harness

Recommended jcode borrow:

- CLI-style smoke tests that do not require model availability;
- tool regression campaigns;
- browser readiness campaigns;
- soft interrupt and checkpoint campaigns.

Keep from uClaw:

- Agent OS-level harness abstraction;
- autonomy promotion gating;
- scorecards connected to runtime projections.

### 11.5 Teams and Subagents

Recommended jcode borrow:

- subagent role clarity;
- batch execution;
- swarm-style workload decomposition;
- background child task status.

Keep from uClaw:

- team specs;
- worker specs;
- main session ownership of edits;
- TaskEvent-based traceability.

## 12. Risk Register

| Risk | Why It Matters | Mitigation |
|---|---|---|
| Literal jcode port creates a second runtime | Users and developers get two sources of truth. | Only port into existing Agent OS v2 spine. |
| Tool migration bypasses safety | Shell, file, network, and browser actions become unsafe. | Every tool capability goes through SafetyManager and policy hooks. |
| Ambient work becomes invisible | Background work may surprise users. | Render scheduled work in WorldProjection with pause/cancel/status. |
| Browser readiness hides failures | Browser tasks fail after user waits. | Add readiness/probe diagnostics before launch. |
| Team runtime becomes untraceable | Subagents produce opaque side effects. | Every worker emits TaskEvent and has harness fixtures. |
| Harness becomes ornamental | Scorecards do not block risky promotion. | Require scorecard evidence for autonomy increases. |
| Broad PRs become unreviewable | Rewrite risk climbs quickly. | One plan, one PR, narrow allowed files, rollback path. |
| Derived code licensing is missed | Legal and NOTICE obligations are violated. | PR-0 and PR plans must classify copy vs adaptation. |

## 13. Definition of Done for Each PR

A PR is done only when:

- the Superpowers skill sequence was followed;
- the plan exists for non-doc implementation work;
- ADR Section 18 answers are present;
- GitNexus impact was run before symbol edits;
- GitNexus detect changes was run before commit;
- tests or harness cases passed;
- rollback is documented;
- DMZ/high-risk review is complete where needed;
- no unrelated user changes were reverted;
- `memory_graph` freeze and home-dir helper rules are preserved.

## 14. Rollback Strategy

Rollback should be boring.

Each implementation PR should be shaped so that disabling one adapter, feature
flag, capability card, or registry entry returns uClaw to the previous path.

For high-risk runtime changes, the PR must include one of:

- a config flag;
- an inactive-by-default registry entry;
- a fallback adapter;
- a migration-free data path;
- an explicit downgrade command or manual rollback checklist.

## 15. Multi-Session Closed Loop

This upgrade is large enough that the project needs a live coordination ledger,
not just a static design doc.

The dedicated ledger is:

```text
docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md
```

It follows the same operating pattern as `docs/superpowers/MILESTONE_STATUS.md`:

- one live Quick View table;
- a decision log;
- per-PR close-loop steps;
- drift alarms;
- handoff and closeout templates;
- explicit next action for the next session.

### 15.1 Per-PR Close Loop

Every PR must update the status file at the start and at close:

1. Set or confirm the current PR row.
2. Record branch/worktree and owner session.
3. Link the plan or spec.
4. Record verification commands.
5. Record GitNexus impact and detect-changes results.
6. Mark the next exact action.

This is intentionally manual. The goal is not automation theater; the goal is
that another session can resume without reconstructing the last transcript.

### 15.2 Multi-Session Handoff

If a session stops mid-PR, it must leave a handoff note with:

- current PR;
- branch/worktree;
- files changed;
- user changes preserved;
- GitNexus impact status;
- tests run;
- known failing tests;
- next exact command;
- decision needed from Ryan;
- rollback path.

### 15.3 Drift Control

Use the status file drift rules to prevent this upgrade from becoming a string
of unrelated tactical fixes.

Red drift signals block new tactical work until the roadmap is re-centered:

- more than five consecutive tactical PRs;
- planned roadmap PR idle for more than fourteen days;
- pilot work older than thirty days without wire-up;
- HIGH/CRITICAL impact work merged without fresh review;
- more than one merged PR without updating the status file.

### 15.4 Closeout Reports

Any PR that closes a major slice should add a closeout report under:

```text
docs/superpowers/reports/
```

The report should include:

- what closed;
- actual vs planned scope;
- harness evidence;
- performance evidence where relevant;
- regressions or escaped risks;
- next recommended PR.

## 16. What PR-0 Does Not Own

PR-0 does not:

- implement runtime changes;
- create migrations;
- edit DMZ files;
- port jcode code;
- decide final type names for PR-1;
- claim a performance win;
- promote any autonomy level;
- change the frontend runtime.

PR-0 only aligns the map before implementation starts.

## 17. Immediate Next Step

After this spec is reviewed, create the first implementation plan:

```text
docs/superpowers/plans/PR-1-runtime-event-spine-audit-and-adapter-contracts.md
```

That plan should be narrow and test-first:

- inspect current TaskEvent, browser task event, automation activity, team
  worker, and harness trace models;
- propose the minimum shared adapter contract;
- add replay fixtures before changing behavior;
- avoid DMZ files unless impact analysis proves there is no cleaner path.

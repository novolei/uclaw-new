# 05. Frontend Integration Plan For A jcode-Grade Backend

Status: analysis document, no implementation changes.
Date: 2026-05-23
Scope: frontend changes needed if uClaw reconstructs its backend around jcode-style Rust modules, protocols, tools, providers, and sessions.

## Executive Judgment

The frontend should not copy jcode's TUI surface. jcode's UI is optimized for terminal interaction; uClaw is a desktop agent workspace with React, Jotai, Tauri events, browser surfaces, previews, safety controls, automation views, Symphony, and Agent OS v2.

The part worth copying is the product protocol discipline:

```text
backend runtime event
  -> typed frontend adapter
  -> session projection reducer
  -> task timeline blocks
  -> domain surfaces
```

The target is:

```text
TaskEvent -> WorldProjection -> UI surfaces
```

This lets uClaw absorb jcode's backend clarity without reducing the app to a terminal transcript.

## Evidence

Primary uClaw frontend evidence:

- `/Users/ryanliu/Documents/uclaw/ui/src/lib/tauri-bridge.ts`
- `/Users/ryanliu/Documents/uclaw/ui/src/atoms/agent-atoms.ts`
- `/Users/ryanliu/Documents/uclaw/ui/src/atoms/chat-atoms.ts`
- `/Users/ryanliu/Documents/uclaw/ui/src/atoms/browser-atoms.ts`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentMessages.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/NativeBlockRenderer.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/ToolActivityItem.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/AskUserBanner.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/PlanModeSuggestBanner.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/browser/BrowserTaskMonitor.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/components/automation/SpecRunSurface.tsx`
- `/Users/ryanliu/Documents/uclaw/ui/src/atoms/safety-atoms.ts`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/ipc.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/contracts.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/automation/mod.rs`
- `/Users/ryanliu/Documents/uclaw/src-tauri/src/browser/mod.rs`

Primary jcode frontend/protocol evidence:

- `/Users/ryanliu/Documents/jcode/src/server.rs`
- `/Users/ryanliu/Documents/jcode/src/server/agent_control.rs`
- `/Users/ryanliu/Documents/jcode/src/server/socket.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-protocol/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-message-types/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-session-types/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-task-types/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-side-panel-types/src/lib.rs`
- `/Users/ryanliu/Documents/jcode/crates/jcode-tui/src/*`
- `/Users/ryanliu/Documents/jcode/docs/DESKTOP_CODEBASE_ARCHITECTURE.md`
- `/Users/ryanliu/Documents/jcode/docs/MULTI_SESSION_CLIENT_ARCHITECTURE.md`
- `/Users/ryanliu/Documents/jcode/docs/SAFETY_SYSTEM.md`

## Current uClaw Frontend Shape

uClaw already has the right product direction:

- Agent messages are rendered as structured UI, not plain terminal output.
- `NativeBlockRenderer` supports ordered native content blocks.
- `ToolActivityItem` gives tool execution a first-class visual surface.
- `AskUserBanner` and `PlanModeSuggestBanner` expose safety and planning boundaries.
- Browser Agent v2 has its own panel and run monitor.
- Automation and spec runs already map backend activity into product views.
- `TaskEvent` is already present in the backend as an Agent OS v2 runtime contract.

The current weakness is fragmentation:

- Agent, Chat, Browser, Automation, and Symphony each have separate event/listener assumptions.
- Tauri bridge types are broad and central, so protocol drift accumulates there.
- Timeline rendering, tool rendering, safety prompts, patch previews, and background work are not yet all projected from one canonical event model.
- Some surfaces are session-oriented while others are view-oriented, but the boundary is implicit.

## What To Copy From jcode

| jcode Pattern | uClaw Frontend Target | Why It Matters | Risk |
|---|---|---|---|
| Typed protocol/event crates | Generated or shared TypeScript runtime event types | Reduces bridge drift and ad hoc event names | Medium |
| Session event stream | `AgentRuntimeEvent` adapter around Tauri listeners | Gives Agent/Chat/Browser/Automation one ingestion path | High |
| Semantic message blocks | `TaskTimelineBlock` model | Avoids treating backend output as markdown-only text | Medium |
| Tool call lifecycle | Tool block reducer and display registry | Makes long tools, approvals, failures, and previews predictable | High |
| Patch/diff events | Patch timeline block plus preview surface | Lets file edits become reviewable product events | High |
| Safety/permission transcript | Boundary timeline block plus pending action state | Keeps approval state auditable and replayable | High |
| Background task events | Substream projection under session timeline | Makes concurrent tool work understandable | Medium |
| Session/surface separation | `SessionRuntimeState` and `SurfaceState` split | Prevents navigation from corrupting runtime truth | Medium |
| Provider/capability metadata | Settings/capability profile display | Makes model routing explainable without leaking backend complexity | Medium |
| Telemetry snapshots | Observability drawer and compact status bars | Helps debug stalls, cost, token, and tool bottlenecks | Medium |

## What Not To Copy

Do not copy jcode's terminal UI layout, keymap, or ANSI display model. uClaw should not regress into a CLI inside a webview.

Do not copy jcode's memory UX directly. uClaw's durable memory direction is gbrain-primary, and `memory_graph` is frozen.

Do not copy jcode's daemon/socket client model as the user-facing frontend contract. Tauri events and local API should remain the desktop shell boundary.

Do not make the frontend mirror every Rust module. The UI needs stable product concepts: session, task, message block, tool block, permission boundary, patch, browser run, automation run, team worker, artifact, and projection.

## Proposed Frontend Architecture

### 1. Runtime event adapter

Create a narrow frontend adapter that translates backend events into a stable UI event union.

Candidate type:

```ts
type AgentRuntimeEvent =
  | { type: 'task.started'; taskId: string; sessionId: string; source: string }
  | { type: 'task.progress'; taskId: string; message: string; detail?: unknown }
  | { type: 'message.delta'; taskId: string; messageId: string; delta: string }
  | { type: 'message.block'; taskId: string; block: TaskTimelineBlock }
  | { type: 'tool.started'; taskId: string; call: ToolCallView }
  | { type: 'tool.progress'; taskId: string; callId: string; progress: ToolProgressView }
  | { type: 'tool.finished'; taskId: string; callId: string; result: ToolResultView }
  | { type: 'permission.requested'; taskId: string; request: PermissionRequestView }
  | { type: 'patch.proposed'; taskId: string; patch: PatchView }
  | { type: 'browser.updated'; taskId: string; run: BrowserRunView }
  | { type: 'automation.updated'; taskId: string; run: AutomationRunView }
  | { type: 'task.finished'; taskId: string; verdict: TaskVerdictView }
```

The names should be finalized against `runtime/contracts.rs`, not copied directly from jcode.

### 2. Session projection reducer

Introduce a reducer that builds a canonical frontend projection:

```ts
interface SessionRuntimeState {
  sessionId: string
  tasks: Record<string, TaskProjection>
  timeline: TaskTimelineBlock[]
  pendingPermissions: Record<string, PermissionRequestView>
  activeTools: Record<string, ToolCallProjection>
  patches: Record<string, PatchProjection>
  telemetry: RuntimeTelemetryProjection
}

interface SurfaceState {
  activePane: 'timeline' | 'browser' | 'preview' | 'automation' | 'team' | 'settings'
  selectedTaskId?: string
  selectedArtifactId?: string
  filters: TimelineFilterState
}
```

This is the key frontend migration. Runtime truth and view selection should not live in the same atom.

### 3. Unified timeline block model

Replace parallel rendering assumptions with a single block model:

```ts
type TaskTimelineBlock =
  | { kind: 'assistant_text'; messageId: string; text: string }
  | { kind: 'native_content'; messageId: string; blocks: NativeContentBlock[] }
  | { kind: 'tool_call'; callId: string; toolName: string; status: ToolStatus }
  | { kind: 'permission'; requestId: string; status: PermissionStatus }
  | { kind: 'patch'; patchId: string; files: PatchFileSummary[] }
  | { kind: 'browser_run'; runId: string; status: BrowserRunStatus }
  | { kind: 'automation_run'; runId: string; status: AutomationRunStatus }
  | { kind: 'team_worker'; workerId: string; status: WorkerStatus }
  | { kind: 'telemetry'; snapshot: RuntimeTelemetryProjection }
```

Existing `NativeBlockRenderer`, `ToolActivityItem`, and browser/automation monitors can become renderers for specific block kinds.

### 4. Tool display registry

Create a frontend registry keyed by tool name and result schema:

```ts
interface ToolDisplayDescriptor {
  toolName: string
  compactTitle(result: ToolResultView): string
  renderSummary(result: ToolResultView): React.ReactNode
  renderDetail?(result: ToolResultView): React.ReactNode
  extractArtifacts?(result: ToolResultView): ArtifactView[]
}
```

This mirrors jcode's tool registry discipline without exposing backend internals in React components.

### 5. Patch and artifact surface

If uClaw imports jcode-style patch/apply semantics, the frontend needs:

- patch proposal block in timeline,
- file summary list,
- changed-line preview,
- approval/reject action where safety policy requires it,
- artifact linkage to preview panels,
- durable receipt in the task event stream.

This must connect to uClaw's path policy and safety receipts, not bypass them.

### 6. Permission boundary surface

Permission requests should be timeline events and pending action state at the same time.

Required fields:

- request id,
- session id,
- task id,
- tool call id when applicable,
- action label,
- risk level,
- path/network/process scope,
- proposed command or operation,
- decision,
- decision time,
- decision source,
- receipt id.

This is stricter than a modal-only approval flow and aligns with uClaw's audit requirements.

### 7. Background work and soft interrupt UX

jcode's background tool and soft interrupt model should become:

- per-task substreams,
- inline progress blocks,
- resumable/cancelable operation controls,
- clear "waiting for user", "background running", and "interrupted" statuses,
- replayable timeline receipts.

Do not hide background work in a generic spinner. The user needs to know which task owns the work and which boundary can stop it.

### 8. Provider and capability UI

If the backend adds jcode-style provider capabilities, routes, failover, and cost metadata, the frontend should expose it as capability profiles:

- selected provider/model,
- supported modalities,
- tool support,
- cache support,
- context window,
- estimated cost,
- fallback route,
- effective safety mode.

This belongs in Settings and compact task telemetry, not in every chat bubble.

## Phased Frontend Migration

### PR-F1: Frontend runtime event adapter

Purpose:

- Create `AgentRuntimeEvent` and adapter functions in `ui/src/lib`.
- Add reducer tests using fixture events.
- Keep existing components rendering from existing atoms.

Verification:

```bash
cd ui && npm test -- --run runtime-event
cd ui && npm test -- --run agent
```

Risk:

- Medium: event naming drift between Rust and TypeScript.

### PR-F2: Session projection atom split

Purpose:

- Split runtime state from view state.
- Keep compatibility selectors for existing components.

Verification:

```bash
cd ui && npm test -- --run agent-atoms
```

Risk:

- High: Agent view, Chat view, browser monitor, and automation surfaces may all depend on current atom shape.

### PR-F3: Timeline block renderer

Purpose:

- Introduce `TaskTimelineBlock`.
- Wrap existing renderers instead of rewriting them.
- Convert `NativeBlockRenderer` and `ToolActivityItem` into block-specific renderers.

Verification:

```bash
cd ui && npm test -- --run NativeBlockRenderer ToolActivityItem
```

Risk:

- Medium: visual regressions and ordering bugs.

### PR-F4: Permission and patch blocks

Purpose:

- Render permission requests and patch proposals as timeline blocks.
- Preserve existing safety mode controls.
- Add file/path summaries that match backend path policy.

Verification:

```bash
cd ui && npm test -- --run permission patch
```

Risk:

- High: this touches user trust, file writes, and approval semantics.

### PR-F5: Background, telemetry, and capability surfaces

Purpose:

- Add background substreams and soft interrupt status.
- Add compact runtime telemetry projection.
- Add capability profile display in settings/task detail.

Verification:

```bash
cd ui && npm test -- --run telemetry capability background
```

Risk:

- Medium: event volume and rendering performance.

### PR-F6: WorldProjection convergence

Purpose:

- Map session projections into World Projection.
- Let Agent, Chat, Browser, Automation, Symphony, and Team views share task truth.

Verification:

```bash
cd ui && npm test -- --run world projection
```

Risk:

- High: this is where frontend product architecture changes become visible.

### PR-F7: Team and worker projection

Purpose:

- Render subagents and teams as workers under the same task projection.
- Replace hidden team chat assumptions with role output, assignment, status, review verdict, and artifact blocks.

Required blocks:

- `worker_assigned`,
- `worker_status`,
- `team_channel_message`,
- `review_gate`,
- `worker_artifact`,
- `team_verdict`.

Verification:

```bash
cd ui && npm test -- --run team worker projection
```

Risk:

- High: current Agent Teams, Symphony, and automation views can drift into separate truth surfaces.

### PR-F8: Browser provider surface

Purpose:

- Keep uClaw's Browser Agent v2 UI as the primary browser experience.
- Add provider status/readiness/capability display inspired by jcode browser metadata.

Required UI states:

- provider ready,
- provider not configured,
- setup/repair required,
- auth profile applied,
- boundary detected,
- checkpoint available,
- resume available,
- provider unsupported action.

Verification:

```bash
cd ui && npm test -- --run browser provider
```

Risk:

- Medium: provider readiness must not be confused with page/task success.

### PR-F9: Scheduled worker and ambient-equivalent surface

Purpose:

- Represent jcode ambient-like background/scheduled work through uClaw automation and heartbeat concepts.

Required UI blocks:

- scheduled wake,
- queued directive,
- active scheduled worker,
- permission boundary,
- user-active pause,
- token/cost headroom,
- next wake,
- completion summary.

Verification:

```bash
cd ui && npm test -- --run automation scheduled worker
```

Risk:

- High: hidden background work can erode trust unless it appears in the same task timeline.

### PR-F10: Harness and scorecard surface

Purpose:

- Show harness evidence for reconstructed tool/browser/team/runtime changes.

Required surfaces:

- harness case list,
- latest episode,
- verdict and score,
- artifacts,
- regression blockers,
- promotion gate decision.

Verification:

```bash
cd ui && npm test -- --run harness scorecard
```

Risk:

- Medium: scorecards become decorative unless linked to PR/promote/rollback decisions.

## Design Requirements

The upgraded UI should remain a work-focused desktop tool:

- dense but scan-friendly,
- no landing-page treatment,
- no decorative panels for their own sake,
- no terminal clone,
- no explanatory instructional copy inside the app unless the user is blocked,
- stable dimensions for timeline rows, tool blocks, and preview panes,
- predictable icon controls for repeated actions,
- compact typography inside tool and task surfaces,
- clear separation between action, status, evidence, and artifacts.

## Verification Strategy

Required automated tests:

- event adapter fixture tests,
- projection reducer tests,
- block ordering tests,
- tool display registry tests,
- permission decision tests,
- patch summary rendering tests,
- browser and automation event mapping tests.
- worker/team event mapping tests,
- browser provider readiness tests,
- scheduled worker projection tests,
- harness scorecard rendering tests.

Required manual or Playwright checks after implementation:

- normal agent turn,
- tool approval,
- rejected tool approval,
- patch proposal,
- long shell command with progress,
- browser task run,
- automation run,
- background task interruption,
- session resume,
- provider fallback or provider error.

## Main Frontend Risk Register

| Risk | Severity | Why | Mitigation |
|---|---:|---|---|
| Event protocol drift | High | Rust and TS may evolve separately | Shared schema tests or generated TS from Rust DTOs |
| Timeline ordering bugs | High | Streaming deltas, tool calls, and approvals interleave | Deterministic reducer with fixture replay tests |
| Permission ambiguity | Critical | Wrong approval UI can cause unsafe writes or commands | Treat permission as auditable event plus pending boundary |
| UI fragmentation remains | High | New backend modules may still feed old atoms directly | One adapter path for all runtime events |
| Rendering jank | Medium | Background/tool telemetry can be noisy | Batch events and virtualize long timelines |
| Over-copying jcode TUI | Medium | Terminal metaphors do not fit uClaw's product | Copy protocol and lifecycle ideas only |
| Memory model confusion | Critical | `memory_graph` is frozen, gbrain is canonical | Display gbrain receipts and avoid memory_graph write flows |
| Team surface drift | High | Team chat, Symphony, and worker status can become separate product truths | Render workers through WorldProjection |
| Browser provider confusion | Medium | Setup/readiness can be mistaken for task completion | Separate provider status, page state, and task verdict blocks |
| Background work invisibility | High | Ambient/scheduled work can feel hidden | Scheduled workers must emit timeline and boundary events |
| Harness as decoration | Medium | Scores that do not gate anything become noise | Link scorecards to promotion and rollback decisions |

## Final Recommendation

Frontend reconstruction should start only after backend PRs define stable event DTOs. The first frontend milestone is not a visual redesign; it is a canonical projection layer that can absorb jcode-style runtime events while preserving uClaw's Agent OS v2 product model.

The right migration slogan is:

> Copy jcode's event discipline. Keep uClaw's product surface.

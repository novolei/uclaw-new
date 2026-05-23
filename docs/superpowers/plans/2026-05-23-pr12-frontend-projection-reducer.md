# PR-12 Frontend Projection Reducer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a pure frontend reducer that turns PR-5 `SessionProjectionStub` and compact journal entries into a stable Agent OS runtime projection view model.

**Architecture:** PR-12 stays pure TypeScript and does not wire Tauri commands, startup boot, or UI components. The reducer mirrors the backend projection DTO shape, normalizes tasks by id, derives status buckets and attention markers, and ignores stale per-task journal updates so later UI surfaces can consume one deterministic projection.

**Tech Stack:** TypeScript, Vitest, existing Vite alias setup, PR-5 backend projection journal DTOs.

---

## Scope Boundaries

- Owns: frontend projection DTO types, reducer helpers, deterministic fixture tests, PR status documentation.
- Does not own: Rust projection journal shape changes, Tauri commands, event listeners, Jotai atoms, Agent/Chat/Browser visual convergence, SQLite, harness runners.
- Design source: `docs/jcode_comparison/05_frontend_integration.md` session projection reducer section and PR-5 `src-tauri/src/runtime/projection_journal.rs`.

## ADR 18 Answers

| Question | PR-12 Answer |
|---|---|
| 1. What user intent does this support? | Let Agent, Chat, Browser, Automation, and Team surfaces render runtime task truth from one stable projection instead of per-panel local guesses. |
| 2. What autonomy level can it run at? | Passive read-only UI projection; it performs no autonomous action. |
| 3. What is the canonical truth source? | Backend PR-5 `TaskEvent`-derived `SessionProjectionStub` and compact `ProjectionJournalEntry` DTOs. |
| 4. What TaskEvent entries does it emit? | None; this PR only consumes already-derived projection DTOs. |
| 5. What context does it read, and how is it cited? | Projection DTOs carry `sourceRolloutFile`, `lastSequence`, timestamps, task ids, and event kind summaries as citation handles back to rollout truth. |
| 6. What capability cards does it add or consume? | Adds no capability cards; later UI surfaces may consume this projection behind existing capability policy. |
| 7. What policy hooks can block it? | None in this pure reducer; future command/listener wiring remains blockable by runtime and capability policy. |
| 8. What world projection does the UI render? | A deterministic task map plus active, waiting, terminal, and attention buckets derived from backend projection records. |
| 9. What harness cases prove it works? | Vitest fixture replay for stub normalization, stale/duplicate journal replay, waiting hydration, terminal transitions, and kebab-case task ids. |
| 10. What is the rollback or disable path? | Remove the new `ui/src/lib/agent-os/projection-reducer*` files and status-plan doc updates; no runtime wiring to disable. |
| 11. What does it deliberately not own? | Rust projection shape, Tauri commands, startup boot, Jotai atoms, visual components, SQLite, and surface convergence. |

## Files

- Create: `ui/src/lib/agent-os/projection-reducer.ts`
- Create: `ui/src/lib/agent-os/projection-reducer.test.ts`
- Modify: `docs/superpowers/plans/2026-05-23-pr12-frontend-projection-reducer.md`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

## Task 1: Reducer Contract Tests

- [x] **Step 1: Add failing fixture tests**

Create `ui/src/lib/agent-os/projection-reducer.test.ts` with tests for:

```ts
import { describe, expect, it } from 'vitest'
import {
  applyProjectionJournalEntries,
  buildRuntimeProjection,
  emptyRuntimeProjection,
  type ProjectionJournalEntry,
  type SessionProjectionStub,
} from './projection-reducer'

const stub: SessionProjectionStub = {
  schemaVersion: 1,
  generatedAt: '2026-05-23T00:00:00Z',
  sourceRolloutFile: '/tmp/rollouts/session.jsonl',
  lastSequence: 3,
  malformedLineCount: 1,
  tasks: [
    {
      taskId: 'task-a',
      intentId: 'intent-a',
      source: 'agent_loop',
      firstTs: '2026-05-23T00:00:01Z',
      lastTs: '2026-05-23T00:00:03Z',
      lastKind: 'checkpoint',
      status: 'checkpointed',
      isTerminal: false,
      eventCount: 2,
      lastSequence: 2,
      checkpointRef: 'ckpt-1',
      warningCount: 0,
      sourceRolloutFile: '/tmp/rollouts/session.jsonl',
    },
    {
      taskId: 'task-b',
      source: 'browser',
      status: 'waiting',
      isTerminal: false,
      eventCount: 1,
      lastSequence: 3,
      boundaryReason: 'needs-login',
      warningCount: 1,
      sourceRolloutFile: '/tmp/rollouts/session.jsonl',
    },
  ],
}

describe('frontend runtime projection reducer', () => {
  it('normalizes a backend stub into deterministic task buckets', () => {
    const projection = buildRuntimeProjection(stub)

    expect(projection.session.generatedAt).toBe(stub.generatedAt)
    expect(projection.taskOrder).toEqual(['task-a', 'task-b'])
    expect(projection.activeTaskIds).toEqual(['task-a'])
    expect(projection.waitingTaskIds).toEqual(['task-b'])
    expect(projection.terminalTaskIds).toEqual([])
    expect(projection.attentionTaskIds).toEqual(['task-b'])
    expect(projection.malformedLineCount).toBe(1)
  })

  it('applies journal entries without accepting stale per-task sequence updates', () => {
    const projection = buildRuntimeProjection(stub)
    const entries: ProjectionJournalEntry[] = [
      { sequence: 1, taskId: 'task-a', ts: 'old', kind: 'task.started', source: 'agent_loop', status: 'running', isTerminal: false },
      { sequence: 4, taskId: 'task-a', ts: '2026-05-23T00:00:04Z', kind: 'task.finished', source: 'agent_loop', status: 'completed', isTerminal: true },
    ]

    const updated = applyProjectionJournalEntries(projection, entries)

    expect(updated.tasks.taskA).toBeUndefined()
    expect(updated.tasks['task-a']?.status).toBe('completed')
    expect(updated.activeTaskIds).toEqual([])
    expect(updated.terminalTaskIds).toEqual(['task-a'])
    expect(updated.lastSequence).toBe(4)
  })

  it('can start from an empty projection for event-only hydration', () => {
    const updated = applyProjectionJournalEntries(emptyRuntimeProjection('session-1'), [
      { sequence: 1, taskId: 'task-c', ts: '2026-05-23T00:00:01Z', kind: 'boundary.yielded', source: 'tasks', status: 'waiting', isTerminal: false, boundaryReason: 'approval' },
    ])

    expect(updated.taskOrder).toEqual(['task-c'])
    expect(updated.waitingTaskIds).toEqual(['task-c'])
    expect(updated.attentionTaskIds).toEqual(['task-c'])
  })
})
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run src/lib/agent-os/projection-reducer.test.ts`

Expected: FAIL because `projection-reducer.ts` does not exist yet. Execution note: the initial missing-module red step was superseded by the subagent handoff, and the duplicate-replay regression was added red/green during review before final verification.

## Task 2: Pure Projection Reducer

- [x] **Step 1: Implement the reducer module**

Create `ui/src/lib/agent-os/projection-reducer.ts` with:

```ts
export type TaskProjectionStatus =
  | 'running'
  | 'waiting'
  | 'checkpointed'
  | 'completed'
  | 'cancelled'
  | 'failed'
  | 'budget_exhausted'

export type TaskEventSource =
  | 'agent_loop'
  | 'browser'
  | 'tools'
  | 'skills'
  | 'plugins'
  | 'permissions'
  | 'hooks'
  | 'memory'
  | 'gbrain'
  | 'tasks'
  | 'coordinator'
  | 'prompts'
  | 'automation'

export interface TaskProjectionSummary {
  taskId: string
  intentId?: string
  source: TaskEventSource
  firstTs?: string
  lastTs?: string
  lastKind?: string
  status: TaskProjectionStatus
  isTerminal: boolean
  eventCount: number
  lastSequence: number
  checkpointRef?: string
  boundaryReason?: string
  warningCount: number
  sourceRolloutFile: string
}

export interface SessionProjectionStub {
  schemaVersion: number
  generatedAt: string
  sourceRolloutFile: string
  lastSequence: number
  malformedLineCount: number
  tasks: TaskProjectionSummary[]
}

export interface ProjectionJournalEntry {
  sequence: number
  taskId: string
  ts: string
  kind: string
  source: TaskEventSource
  status: TaskProjectionStatus
  isTerminal: boolean
  checkpointRef?: string
  boundaryReason?: string
}

export interface RuntimeProjection {
  session: {
    sessionId?: string
    schemaVersion: number
    generatedAt?: string
    sourceRolloutFile?: string
  }
  tasks: Record<string, TaskProjectionSummary>
  taskOrder: string[]
  activeTaskIds: string[]
  waitingTaskIds: string[]
  terminalTaskIds: string[]
  attentionTaskIds: string[]
  lastSequence: number
  malformedLineCount: number
}
```

Implementation rules:

- `buildRuntimeProjection(stub, options?)` copies backend stub data without mutating the input.
- `emptyRuntimeProjection(sessionId?)` creates an empty projection for future event-only hydration.
- `applyProjectionJournalEntries(projection, entries)` updates only tasks whose entry sequence is newer than the task's current `lastSequence`; equal sequence entries are duplicate replay and must be ignored.
- Missing tasks created from journal entries receive `eventCount: 1`, `firstTs: entry.ts`, and `sourceRolloutFile` from the projection session when available.
- Buckets are derived after every reducer pass:
  - active: `running` or `checkpointed`
  - waiting: `waiting`
  - terminal: `isTerminal`
  - attention: waiting, warning count > 0, or failed/budget exhausted

- [x] **Step 2: Run focused tests**

Run: `cd ui && npm test -- --run src/lib/agent-os/projection-reducer.test.ts`

Expected: PASS for all projection reducer tests.

## Task 3: Docs, Verification, and Commit

- [x] **Step 1: Mark PR12 status**

Update `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md` so PR12 is in progress while coding, then Open after the GitHub PR is created.

- [x] **Step 2: Run final verification**

Run:

```bash
cd ui && npm test -- --run src/lib/agent-os/projection-reducer.test.ts
git diff --check -- docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-pr12-frontend-projection-reducer.md ui/src/lib/agent-os/projection-reducer.ts ui/src/lib/agent-os/projection-reducer.test.ts
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr12-frontend-projection
```

Expected:

- Vitest passes.
- Diff whitespace check passes.
- GitNexus reports only the new frontend projection reducer and docs/status files.

- [x] **Step 3: Commit**

Commit message:

```bash
git commit -m "feat(ui): add Agent OS projection reducer"
```

Commit body must include the verification command and expected passing output.

## Self-Review

- Spec coverage: covers the PR-12 frontend reducer slice from the Agent OS spine and jcode frontend report, while deferring visual surface convergence to PR-13.
- Placeholder scan: no TODO/TBD placeholders.
- Type consistency: DTO field names match backend serde camelCase output from PR-5.

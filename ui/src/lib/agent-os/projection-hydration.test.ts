import { describe, expect, it } from 'vitest'
import type {
  ProjectionJournalEntry,
  SessionProjectionStub,
  TaskProjectionSummary,
} from './projection-reducer'
import {
  applyProjectionHydration,
  hydrateProjection,
  type ProjectionHydrationPayload,
} from './projection-hydration'

function task(overrides: Partial<TaskProjectionSummary>): TaskProjectionSummary {
  return {
    taskId: 'task-default',
    source: 'agent_loop',
    status: 'running',
    isTerminal: false,
    eventCount: 1,
    lastSequence: 1,
    warningCount: 0,
    sourceRolloutFile: '/tmp/session.rollout.jsonl',
    ...overrides,
  }
}

function entry(overrides: Partial<ProjectionJournalEntry>): ProjectionJournalEntry {
  return {
    sequence: 1,
    taskId: 'task-default',
    ts: '2026-05-23T12:00:00Z',
    kind: 'task_started',
    source: 'agent_loop',
    status: 'running',
    isTerminal: false,
    ...overrides,
  }
}

function stub(tasks: TaskProjectionSummary[]): SessionProjectionStub {
  return {
    schemaVersion: 1,
    generatedAt: '2026-05-23T12:00:00Z',
    sourceRolloutFile: '/tmp/session.rollout.jsonl',
    lastSequence: Math.max(0, ...tasks.map((item) => item.lastSequence)),
    malformedLineCount: 1,
    tasks,
  }
}

describe('Agent OS projection hydration bridge', () => {
  it('hydrates runtime and world projections from a backend stub', () => {
    const payload: ProjectionHydrationPayload = {
      sessionId: 'session-1',
      stub: stub([
        task({ taskId: 'agent-task', source: 'agent_loop', lastSequence: 1 }),
        task({
          taskId: 'browser-task',
          source: 'browser',
          status: 'waiting',
          boundaryReason: 'login',
          lastSequence: 2,
        }),
      ]),
      journalEntries: [],
    }

    const result = hydrateProjection(payload)

    expect(result.runtime.session.sessionId).toBe('session-1')
    expect(result.runtime.taskOrder).toEqual(['agent-task', 'browser-task'])
    expect(result.runtime.waitingTaskIds).toEqual(['browser-task'])
    expect(result.world.surfaces.agent.taskIds).toEqual(['agent-task', 'browser-task'])
    expect(result.world.surfaces.browser.waitingTaskIds).toEqual(['browser-task'])
    expect(result.source).toEqual({
      hasStub: true,
      journalEntryCount: 0,
    })
  })

  it('replays journal entries after the stub and rebuilds world surfaces', () => {
    const result = hydrateProjection({
      sessionId: 'session-2',
      stub: stub([
        task({ taskId: 'agent-task', source: 'agent_loop', lastSequence: 1 }),
      ]),
      journalEntries: [
        entry({
          sequence: 2,
          taskId: 'agent-task',
          ts: '2026-05-23T12:01:00Z',
          kind: 'task_finished',
          status: 'completed',
          isTerminal: true,
        }),
        entry({
          sequence: 3,
          taskId: 'automation-task',
          ts: '2026-05-23T12:02:00Z',
          kind: 'boundary_yield',
          source: 'automation',
          status: 'waiting',
          boundaryReason: 'approval',
        }),
      ],
    })

    expect(result.runtime.terminalTaskIds).toEqual(['agent-task'])
    expect(result.runtime.waitingTaskIds).toEqual(['automation-task'])
    expect(result.world.surfaces.automation.waitingTaskIds).toEqual(['automation-task'])
    expect(result.world.surfaces.symphony.waitingTaskIds).toEqual(['automation-task'])
    expect(result.world.surfaces.agent.terminalTaskIds).toEqual(['agent-task'])
  })

  it('starts from an empty projection when the backend has no stub yet', () => {
    const result = hydrateProjection({
      sessionId: 'session-empty',
      journalEntries: [],
    })

    expect(result.runtime.session).toEqual({
      sessionId: 'session-empty',
      schemaVersion: 1,
    })
    expect(result.runtime.taskOrder).toEqual([])
    expect(result.world.totals.taskCount).toBe(0)
    expect(result.source).toEqual({
      hasStub: false,
      journalEntryCount: 0,
    })
  })

  it('applies incremental hydration onto a prior result without replaying stale entries', () => {
    const initial = hydrateProjection({
      sessionId: 'session-3',
      stub: stub([
        task({ taskId: 'team-task', source: 'tasks', lastSequence: 4 }),
      ]),
    })

    const updated = applyProjectionHydration(initial, {
      sessionId: 'session-3',
      journalEntries: [
        entry({
          sequence: 4,
          taskId: 'team-task',
          ts: 'old',
          source: 'tasks',
          status: 'waiting',
          boundaryReason: 'stale',
        }),
        entry({
          sequence: 5,
          taskId: 'team-task',
          ts: '2026-05-23T12:05:00Z',
          kind: 'checkpoint',
          source: 'tasks',
          status: 'checkpointed',
          checkpointRef: 'ckpt-team',
        }),
      ],
    })

    expect(updated.runtime.tasks['team-task']).toMatchObject({
      status: 'checkpointed',
      eventCount: 2,
      lastSequence: 5,
      checkpointRef: 'ckpt-team',
      boundaryReason: undefined,
    })
    expect(updated.world.surfaces.team.activeTaskIds).toEqual(['team-task'])
    expect(updated.source).toEqual({
      hasStub: false,
      journalEntryCount: 2,
    })
  })

  it('resets journal-only hydration when the payload belongs to a different session', () => {
    const initial = hydrateProjection({
      sessionId: 'session-a',
      stub: stub([
        task({ taskId: 'old-task', source: 'agent_loop', lastSequence: 1 }),
      ]),
    })

    const updated = applyProjectionHydration(initial, {
      sessionId: 'session-b',
      journalEntries: [
        entry({
          sequence: 1,
          taskId: 'new-task',
          ts: '2026-05-23T12:06:00Z',
          source: 'browser',
          status: 'running',
        }),
      ],
    })

    expect(updated.runtime.session.sessionId).toBe('session-b')
    expect(updated.runtime.taskOrder).toEqual(['new-task'])
    expect(updated.runtime.tasks['old-task']).toBeUndefined()
    expect(updated.world.surfaces.browser.activeTaskIds).toEqual(['new-task'])
  })
})

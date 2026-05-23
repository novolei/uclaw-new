import { describe, expect, it } from 'vitest'
import {
  applyProjectionJournalEntries,
  buildRuntimeProjection,
  emptyRuntimeProjection,
  type ProjectionJournalEntry,
  type SessionProjectionStub,
  type TaskProjectionSummary,
} from './projection-reducer'

function task(overrides: Partial<TaskProjectionSummary>): TaskProjectionSummary {
  return {
    taskId: 'task-default',
    source: 'agent_loop',
    status: 'running',
    isTerminal: false,
    eventCount: 1,
    lastSequence: 1,
    warningCount: 0,
    sourceRolloutFile: '/tmp/rollout.jsonl',
    ...overrides,
  }
}

function entry(overrides: Partial<ProjectionJournalEntry>): ProjectionJournalEntry {
  return {
    sequence: 1,
    taskId: 'task-default',
    ts: '2026-05-23T10:00:00Z',
    kind: 'task_started',
    source: 'agent_loop',
    status: 'running',
    isTerminal: false,
    ...overrides,
  }
}

describe('Agent OS projection reducer', () => {
  it('normalizes a backend stub and derives runtime buckets', () => {
    const stub: SessionProjectionStub = {
      schemaVersion: 1,
      generatedAt: '2026-05-23T10:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
      lastSequence: 8,
      malformedLineCount: 2,
      tasks: [
        task({
          taskId: 'task-running',
          status: 'running',
          lastSequence: 4,
        }),
        task({
          taskId: 'task-waiting',
          status: 'waiting',
          lastSequence: 5,
          boundaryReason: 'needs_user_input',
        }),
        task({
          taskId: 'task-checkpointed',
          status: 'checkpointed',
          lastSequence: 6,
          checkpointRef: 'checkpoint-1',
        }),
        task({
          taskId: 'task-completed',
          status: 'completed',
          isTerminal: true,
          lastSequence: 7,
        }),
        task({
          taskId: 'task-warning',
          status: 'running',
          warningCount: 1,
          lastSequence: 8,
        }),
      ],
    }

    const projection = buildRuntimeProjection(stub, { sessionId: 'session-1' })

    expect(projection.session).toEqual({
      sessionId: 'session-1',
      schemaVersion: 1,
      generatedAt: '2026-05-23T10:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
    })
    expect(Object.keys(projection.tasks)).toEqual([
      'task-running',
      'task-waiting',
      'task-checkpointed',
      'task-completed',
      'task-warning',
    ])
    expect(projection.taskOrder).toEqual([
      'task-running',
      'task-waiting',
      'task-checkpointed',
      'task-completed',
      'task-warning',
    ])
    expect(projection.activeTaskIds).toEqual([
      'task-running',
      'task-checkpointed',
      'task-warning',
    ])
    expect(projection.waitingTaskIds).toEqual(['task-waiting'])
    expect(projection.terminalTaskIds).toEqual(['task-completed'])
    expect(projection.attentionTaskIds).toEqual(['task-waiting', 'task-warning'])
    expect(projection.malformedLineCount).toBe(2)
    expect(stub.tasks[0]).not.toBe(projection.tasks['task-running'])
  })

  it('ignores stale per-task entries and applies newer terminal entries', () => {
    const projection = buildRuntimeProjection({
      schemaVersion: 1,
      generatedAt: '2026-05-23T10:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
      lastSequence: 10,
      malformedLineCount: 0,
      tasks: [
        task({
          taskId: 'task-a',
          firstTs: '2026-05-23T10:00:00Z',
          lastTs: '2026-05-23T10:01:00Z',
          lastKind: 'boundary_yield',
          status: 'waiting',
          eventCount: 3,
          lastSequence: 10,
          boundaryReason: 'needs_login',
        }),
      ],
    })

    const next = applyProjectionJournalEntries(projection, [
      entry({
        sequence: 9,
        taskId: 'task-a',
        ts: '2026-05-23T10:02:00Z',
        kind: 'checkpoint',
        status: 'checkpointed',
        checkpointRef: 'stale-checkpoint',
      }),
      entry({
        sequence: 11,
        taskId: 'task-a',
        ts: '2026-05-23T10:03:00Z',
        kind: 'task_finished',
        status: 'completed',
        isTerminal: true,
      }),
    ])

    expect(next.tasks['task-a']).toMatchObject({
      taskId: 'task-a',
      status: 'completed',
      isTerminal: true,
      eventCount: 4,
      lastSequence: 11,
      lastTs: '2026-05-23T10:03:00Z',
      lastKind: 'task_finished',
      boundaryReason: 'needs_login',
    })
    expect(next.tasks.taskA).toBeUndefined()
    expect(Object.keys(next.tasks)).toEqual(['task-a'])
    expect(next.activeTaskIds).toEqual([])
    expect(next.waitingTaskIds).toEqual([])
    expect(next.terminalTaskIds).toEqual(['task-a'])
    expect(next.attentionTaskIds).toEqual([])
  })

  it('hydrates an empty projection from a waiting boundary journal entry', () => {
    const projection = emptyRuntimeProjection('session-2')
    const next = applyProjectionJournalEntries(projection, [
      entry({
        sequence: 4,
        taskId: 'task-boundary',
        ts: '2026-05-23T10:04:00Z',
        kind: 'boundary_yield',
        source: 'tasks',
        status: 'waiting',
        boundaryReason: 'awaiting_approval',
      }),
    ])

    expect(next.session).toEqual({
      sessionId: 'session-2',
      schemaVersion: 1,
    })
    expect(next.taskOrder).toEqual(['task-boundary'])
    expect(next.tasks['task-boundary']).toEqual({
      taskId: 'task-boundary',
      source: 'tasks',
      firstTs: '2026-05-23T10:04:00Z',
      lastTs: '2026-05-23T10:04:00Z',
      lastKind: 'boundary_yield',
      status: 'waiting',
      isTerminal: false,
      eventCount: 1,
      lastSequence: 4,
      checkpointRef: undefined,
      boundaryReason: 'awaiting_approval',
      warningCount: 0,
      sourceRolloutFile: '',
    })
    expect(next.activeTaskIds).toEqual([])
    expect(next.waitingTaskIds).toEqual(['task-boundary'])
    expect(next.terminalTaskIds).toEqual([])
    expect(next.attentionTaskIds).toEqual(['task-boundary'])
  })

  it('preserves an existing boundary reason unless a checkpoint entry replaces it', () => {
    const projection = buildRuntimeProjection({
      schemaVersion: 1,
      generatedAt: '2026-05-23T10:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
      lastSequence: 3,
      malformedLineCount: 0,
      tasks: [
        task({
          taskId: 'task-preserve',
          status: 'waiting',
          eventCount: 2,
          lastSequence: 3,
          boundaryReason: 'needs_credentials',
        }),
      ],
    })

    const checkpointed = applyProjectionJournalEntries(projection, [
      entry({
        sequence: 4,
        taskId: 'task-preserve',
        kind: 'checkpoint',
        status: 'checkpointed',
        checkpointRef: 'checkpoint-2',
      }),
    ])

    expect(checkpointed.tasks['task-preserve']).toMatchObject({
      status: 'checkpointed',
      checkpointRef: 'checkpoint-2',
      boundaryReason: 'needs_credentials',
    })
    expect(checkpointed.activeTaskIds).toEqual(['task-preserve'])
    expect(checkpointed.attentionTaskIds).toEqual([])

    const replaced = applyProjectionJournalEntries(checkpointed, [
      entry({
        sequence: 5,
        taskId: 'task-preserve',
        kind: 'checkpoint',
        status: 'checkpointed',
        checkpointRef: 'checkpoint-3',
        boundaryReason: 'new_boundary',
      }),
    ])

    expect(replaced.tasks['task-preserve']).toMatchObject({
      checkpointRef: 'checkpoint-3',
      boundaryReason: 'new_boundary',
    })
  })

  it('does not double-apply duplicate replay entries with the same sequence', () => {
    const projection = buildRuntimeProjection({
      schemaVersion: 1,
      generatedAt: '2026-05-23T10:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
      lastSequence: 2,
      malformedLineCount: 0,
      tasks: [
        task({
          taskId: 'task-replay',
          status: 'running',
          eventCount: 2,
          lastSequence: 2,
        }),
      ],
    })

    const replayed = applyProjectionJournalEntries(projection, [
      entry({
        sequence: 2,
        taskId: 'task-replay',
        ts: '2026-05-23T10:10:00Z',
        kind: 'checkpoint',
        status: 'checkpointed',
        checkpointRef: 'duplicate-checkpoint',
      }),
    ])

    expect(replayed.tasks['task-replay']).toMatchObject({
      status: 'running',
      eventCount: 2,
      lastSequence: 2,
    })
    expect(replayed.tasks['task-replay']?.checkpointRef).toBeUndefined()
  })
})

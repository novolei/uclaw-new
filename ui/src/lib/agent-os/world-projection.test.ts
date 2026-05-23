import { describe, expect, it } from 'vitest'
import type { RuntimeProjection, TaskProjectionSummary } from './projection-reducer'
import {
  buildWorldProjection,
  type WorldProjectionOptions,
} from './world-projection'

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

function runtimeProjection(tasks: TaskProjectionSummary[]): RuntimeProjection {
  return {
    session: {
      sessionId: 'session-1',
      schemaVersion: 1,
      generatedAt: '2026-05-23T12:00:00Z',
      sourceRolloutFile: '/tmp/session.rollout.jsonl',
    },
    tasks: Object.fromEntries(tasks.map((item) => [item.taskId, item])),
    taskOrder: tasks.map((item) => item.taskId),
    activeTaskIds: tasks
      .filter((item) => !item.isTerminal && (item.status === 'running' || item.status === 'checkpointed'))
      .map((item) => item.taskId),
    waitingTaskIds: tasks
      .filter((item) => !item.isTerminal && item.status === 'waiting')
      .map((item) => item.taskId),
    terminalTaskIds: tasks.filter((item) => item.isTerminal).map((item) => item.taskId),
    attentionTaskIds: tasks
      .filter((item) =>
        item.status === 'waiting'
        || item.warningCount > 0
        || item.status === 'failed'
        || item.status === 'budget_exhausted'
      )
      .map((item) => item.taskId),
    lastSequence: Math.max(0, ...tasks.map((item) => item.lastSequence)),
    malformedLineCount: 1,
  }
}

describe('Agent OS world projection selectors', () => {
  it('maps runtime tasks into stable per-surface task lists', () => {
    const runtime = runtimeProjection([
      task({ taskId: 'agent-task', source: 'agent_loop', lastSequence: 1 }),
      task({ taskId: 'tool-task', source: 'tools', lastSequence: 2 }),
      task({ taskId: 'browser-task', source: 'browser', lastSequence: 3 }),
      task({ taskId: 'automation-task', source: 'automation', lastSequence: 4 }),
      task({ taskId: 'team-task', source: 'tasks', lastSequence: 5 }),
      task({ taskId: 'coordinator-task', source: 'coordinator', lastSequence: 6 }),
    ])

    const world = buildWorldProjection(runtime)

    expect(world.taskOrder).toEqual([
      'agent-task',
      'tool-task',
      'browser-task',
      'automation-task',
      'team-task',
      'coordinator-task',
    ])
    expect(world.surfaces.agent.taskIds).toEqual(world.taskOrder)
    expect(world.surfaces.chat.taskIds).toEqual(['agent-task', 'tool-task'])
    expect(world.surfaces.browser.taskIds).toEqual(['browser-task'])
    expect(world.surfaces.automation.taskIds).toEqual(['automation-task'])
    expect(world.surfaces.team.taskIds).toEqual(['team-task', 'coordinator-task'])
    expect(world.surfaces.symphony.taskIds).toEqual([
      'automation-task',
      'team-task',
      'coordinator-task',
    ])
  })

  it('derives status buckets independently for every surface', () => {
    const runtime = runtimeProjection([
      task({ taskId: 'agent-active', source: 'agent_loop', status: 'running', lastSequence: 1 }),
      task({
        taskId: 'browser-waiting',
        source: 'browser',
        status: 'waiting',
        boundaryReason: 'needs-login',
        lastSequence: 2,
      }),
      task({
        taskId: 'automation-done',
        source: 'automation',
        status: 'completed',
        isTerminal: true,
        lastSequence: 3,
      }),
      task({
        taskId: 'team-failed',
        source: 'tasks',
        status: 'failed',
        isTerminal: true,
        lastSequence: 4,
      }),
    ])

    const world = buildWorldProjection(runtime)

    expect(world.surfaces.agent.activeTaskIds).toEqual(['agent-active'])
    expect(world.surfaces.agent.waitingTaskIds).toEqual(['browser-waiting'])
    expect(world.surfaces.agent.terminalTaskIds).toEqual(['automation-done', 'team-failed'])
    expect(world.surfaces.browser.waitingTaskIds).toEqual(['browser-waiting'])
    expect(world.surfaces.automation.terminalTaskIds).toEqual(['automation-done'])
    expect(world.surfaces.team.terminalTaskIds).toEqual(['team-failed'])
    expect(world.surfaces.agent.attentionTaskIds).toEqual(['browser-waiting', 'team-failed'])
  })

  it('normalizes task role and attention reason for surface renderers', () => {
    const runtime = runtimeProjection([
      task({ taskId: 'memory-warning', source: 'gbrain', warningCount: 1, lastSequence: 1 }),
      task({
        taskId: 'budget-stop',
        source: 'agent_loop',
        status: 'budget_exhausted',
        isTerminal: true,
        lastSequence: 2,
      }),
      task({
        taskId: 'permission-wait',
        source: 'permissions',
        status: 'waiting',
        boundaryReason: 'approval',
        lastSequence: 3,
      }),
    ])

    const world = buildWorldProjection(runtime)

    expect(world.tasks['memory-warning']).toMatchObject({
      role: 'memory',
      attention: 'warning',
    })
    expect(world.tasks['budget-stop']).toMatchObject({
      role: 'primary',
      attention: 'budget_exhausted',
    })
    expect(world.tasks['permission-wait']).toMatchObject({
      role: 'tool',
      attention: 'waiting',
      surfaces: ['agent', 'chat'],
    })
  })

  it('allows explicit surface overrides without mutating the runtime projection', () => {
    const runtime = runtimeProjection([
      task({ taskId: 'custom-task', source: 'browser', lastSequence: 1 }),
    ])
    const options: WorldProjectionOptions = {
      surfaceOverrides: {
        browser: ['agent', 'browser', 'team'],
      },
    }

    const world = buildWorldProjection(runtime, options)

    expect(world.surfaces.team.taskIds).toEqual(['custom-task'])
    expect(world.tasks['custom-task']?.surfaces).toEqual(['agent', 'browser', 'team'])
    expect(runtime.tasks['custom-task']?.source).toBe('browser')
  })
})

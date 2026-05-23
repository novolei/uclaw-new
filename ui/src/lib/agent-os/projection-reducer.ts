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

export interface BuildRuntimeProjectionOptions {
  sessionId?: string
}

export function emptyRuntimeProjection(sessionId?: string): RuntimeProjection {
  return deriveBuckets({
    session: {
      sessionId,
      schemaVersion: 1,
    },
    tasks: {},
    taskOrder: [],
    activeTaskIds: [],
    waitingTaskIds: [],
    terminalTaskIds: [],
    attentionTaskIds: [],
    lastSequence: 0,
    malformedLineCount: 0,
  })
}

export function buildRuntimeProjection(
  stub: SessionProjectionStub,
  options: BuildRuntimeProjectionOptions = {},
): RuntimeProjection {
  const tasks: Record<string, TaskProjectionSummary> = {}
  const taskOrder: string[] = []

  for (const task of stub.tasks) {
    if (!task.taskId) continue
    const taskId = task.taskId
    if (!Object.prototype.hasOwnProperty.call(tasks, taskId)) {
      taskOrder.push(taskId)
    }
    tasks[taskId] = { ...task }
  }

  return deriveBuckets({
    session: {
      sessionId: options.sessionId,
      schemaVersion: stub.schemaVersion,
      generatedAt: stub.generatedAt,
      sourceRolloutFile: stub.sourceRolloutFile,
    },
    tasks,
    taskOrder,
    activeTaskIds: [],
    waitingTaskIds: [],
    terminalTaskIds: [],
    attentionTaskIds: [],
    lastSequence: stub.lastSequence,
    malformedLineCount: stub.malformedLineCount,
  })
}

export function applyProjectionJournalEntries(
  projection: RuntimeProjection,
  entries: ProjectionJournalEntry[],
): RuntimeProjection {
  const tasks: Record<string, TaskProjectionSummary> = {}
  for (const [taskId, task] of Object.entries(projection.tasks)) {
    tasks[taskId] = { ...task }
  }

  const taskOrder = [...projection.taskOrder]
  let lastSequence = projection.lastSequence

  for (const entry of entries) {
    const existing = tasks[entry.taskId]
    if (existing && entry.sequence <= existing.lastSequence) {
      continue
    }

    if (!existing) {
      tasks[entry.taskId] = {
        taskId: entry.taskId,
        source: entry.source,
        firstTs: entry.ts,
        lastTs: entry.ts,
        lastKind: entry.kind,
        status: entry.status,
        isTerminal: entry.isTerminal,
        eventCount: 1,
        lastSequence: entry.sequence,
        checkpointRef: entry.checkpointRef,
        boundaryReason: entry.boundaryReason,
        warningCount: 0,
        sourceRolloutFile: projection.session.sourceRolloutFile ?? '',
      }
      taskOrder.push(entry.taskId)
      lastSequence = Math.max(lastSequence, entry.sequence)
      continue
    }

    tasks[entry.taskId] = {
      ...existing,
      source: entry.source,
      lastTs: entry.ts,
      lastKind: entry.kind,
      status: entry.status,
      isTerminal: entry.isTerminal,
      eventCount: existing.eventCount + 1,
      lastSequence: entry.sequence,
      checkpointRef: entry.checkpointRef ?? existing.checkpointRef,
      boundaryReason: entry.boundaryReason ?? existing.boundaryReason,
    }
    lastSequence = Math.max(lastSequence, entry.sequence)
  }

  return deriveBuckets({
    ...projection,
    tasks,
    taskOrder,
    lastSequence,
  })
}

function deriveBuckets(projection: RuntimeProjection): RuntimeProjection {
  const activeTaskIds: string[] = []
  const waitingTaskIds: string[] = []
  const terminalTaskIds: string[] = []
  const attentionTaskIds: string[] = []

  for (const taskId of projection.taskOrder) {
    const task = projection.tasks[taskId]
    if (!task) continue

    if (task.isTerminal) {
      terminalTaskIds.push(taskId)
    }
    if (!task.isTerminal && (task.status === 'running' || task.status === 'checkpointed')) {
      activeTaskIds.push(taskId)
    }
    if (!task.isTerminal && task.status === 'waiting') {
      waitingTaskIds.push(taskId)
    }
    if (
      task.status === 'waiting' ||
      task.warningCount > 0 ||
      task.status === 'failed' ||
      task.status === 'budget_exhausted'
    ) {
      attentionTaskIds.push(taskId)
    }
  }

  return {
    ...projection,
    activeTaskIds,
    waitingTaskIds,
    terminalTaskIds,
    attentionTaskIds,
  }
}

import type {
  RuntimeProjection,
  TaskEventSource,
  TaskProjectionStatus,
  TaskProjectionSummary,
} from './projection-reducer'

export type WorldSurface = 'agent' | 'chat' | 'browser' | 'automation' | 'symphony' | 'team'

export type WorldTaskRole =
  | 'primary'
  | 'tool'
  | 'browser'
  | 'automation'
  | 'memory'
  | 'coordination'
  | 'worker'

export type WorldTaskAttention = 'none' | 'waiting' | 'warning' | 'failed' | 'budget_exhausted'

export interface WorldSurfaceProjection {
  taskIds: string[]
  activeTaskIds: string[]
  waitingTaskIds: string[]
  terminalTaskIds: string[]
  attentionTaskIds: string[]
}

export interface WorldTaskProjection extends TaskProjectionSummary {
  role: WorldTaskRole
  attention: WorldTaskAttention
  surfaces: WorldSurface[]
}

export interface WorldProjection {
  session: RuntimeProjection['session']
  tasks: Record<string, WorldTaskProjection>
  taskOrder: string[]
  surfaces: Record<WorldSurface, WorldSurfaceProjection>
  totals: {
    taskCount: number
    activeCount: number
    waitingCount: number
    terminalCount: number
    attentionCount: number
    malformedLineCount: number
    lastSequence: number
  }
}

export interface WorldProjectionOptions {
  surfaceOverrides?: Partial<Record<TaskEventSource, WorldSurface[]>>
}

const WORLD_SURFACES: WorldSurface[] = ['agent', 'chat', 'browser', 'automation', 'symphony', 'team']

export function buildWorldProjection(
  runtime: RuntimeProjection,
  options: WorldProjectionOptions = {},
): WorldProjection {
  const tasks: Record<string, WorldTaskProjection> = {}
  const surfaces = emptySurfaceMap()

  for (const taskId of runtime.taskOrder) {
    const task = runtime.tasks[taskId]
    if (!task) continue

    const mappedSurfaces = surfacesForTask(task, options)
    const worldTask: WorldTaskProjection = {
      ...task,
      role: roleForSource(task.source),
      attention: attentionForTask(task),
      surfaces: mappedSurfaces,
    }
    tasks[taskId] = worldTask

    for (const surface of mappedSurfaces) {
      addTaskToSurface(surfaces[surface], worldTask)
    }
  }

  return {
    session: { ...runtime.session },
    tasks,
    taskOrder: runtime.taskOrder.filter((taskId) => Boolean(tasks[taskId])),
    surfaces,
    totals: {
      taskCount: Object.keys(tasks).length,
      activeCount: runtime.activeTaskIds.length,
      waitingCount: runtime.waitingTaskIds.length,
      terminalCount: runtime.terminalTaskIds.length,
      attentionCount: runtime.attentionTaskIds.length,
      malformedLineCount: runtime.malformedLineCount,
      lastSequence: runtime.lastSequence,
    },
  }
}

function emptySurfaceMap(): Record<WorldSurface, WorldSurfaceProjection> {
  return {
    agent: emptySurfaceProjection(),
    chat: emptySurfaceProjection(),
    browser: emptySurfaceProjection(),
    automation: emptySurfaceProjection(),
    symphony: emptySurfaceProjection(),
    team: emptySurfaceProjection(),
  }
}

function emptySurfaceProjection(): WorldSurfaceProjection {
  return {
    taskIds: [],
    activeTaskIds: [],
    waitingTaskIds: [],
    terminalTaskIds: [],
    attentionTaskIds: [],
  }
}

function addTaskToSurface(surface: WorldSurfaceProjection, task: WorldTaskProjection): void {
  surface.taskIds.push(task.taskId)
  if (task.isTerminal) {
    surface.terminalTaskIds.push(task.taskId)
  } else if (task.status === 'waiting') {
    surface.waitingTaskIds.push(task.taskId)
  } else if (task.status === 'running' || task.status === 'checkpointed') {
    surface.activeTaskIds.push(task.taskId)
  }
  if (task.attention !== 'none') {
    surface.attentionTaskIds.push(task.taskId)
  }
}

function surfacesForTask(
  task: TaskProjectionSummary,
  options: WorldProjectionOptions,
): WorldSurface[] {
  const override = options.surfaceOverrides?.[task.source]
  if (override) return dedupeSurfaces(override)

  const surfaces: WorldSurface[] = ['agent']
  switch (task.source) {
    case 'agent_loop':
    case 'tools':
    case 'skills':
    case 'plugins':
    case 'permissions':
    case 'hooks':
    case 'memory':
    case 'gbrain':
    case 'prompts':
      surfaces.push('chat')
      break
    case 'browser':
      surfaces.push('browser')
      break
    case 'automation':
      surfaces.push('automation', 'symphony')
      break
    case 'tasks':
    case 'coordinator':
      surfaces.push('team', 'symphony')
      break
  }
  return dedupeSurfaces(surfaces)
}

function dedupeSurfaces(surfaces: WorldSurface[]): WorldSurface[] {
  const result: WorldSurface[] = []
  for (const surface of surfaces) {
    if (!result.includes(surface)) result.push(surface)
  }
  return result
}

function roleForSource(source: TaskEventSource): WorldTaskRole {
  switch (source) {
    case 'browser':
      return 'browser'
    case 'automation':
      return 'automation'
    case 'memory':
    case 'gbrain':
      return 'memory'
    case 'tasks':
      return 'worker'
    case 'coordinator':
      return 'coordination'
    case 'tools':
    case 'skills':
    case 'plugins':
    case 'permissions':
    case 'hooks':
      return 'tool'
    case 'agent_loop':
    case 'prompts':
      return 'primary'
  }
}

function attentionForTask(task: {
  status: TaskProjectionStatus
  warningCount: number
}): WorldTaskAttention {
  if (task.status === 'waiting') return 'waiting'
  if (task.status === 'failed') return 'failed'
  if (task.status === 'budget_exhausted') return 'budget_exhausted'
  if (task.warningCount > 0) return 'warning'
  return 'none'
}

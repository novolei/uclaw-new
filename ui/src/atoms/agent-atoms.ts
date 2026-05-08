/**
 * Agent Atoms — Agent 模式的 Jotai 状态管理
 *
 * 管理 Agent 会话列表、当前会话、消息、流式状态等。
 * 从 Proma 迁移，IPC 层由 tauri-bridge.ts 提供。
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import type {
  AgentSessionMeta,
  AgentMessage,
  AgentEvent,
  AgentWorkspace,
  AgentPendingFile,
  RetryAttempt,
  PromaPermissionMode,
  PermissionRequest,
  AskUserRequest,
  ExitPlanModeRequest,
  ThinkingConfig,
  AgentEffort,
  TaskUsage,
  SDKMessage,
} from '@/lib/proma-types'

/** 活动状态 */
export type ActivityStatus = 'pending' | 'running' | 'completed' | 'error' | 'backgrounded'

/** 工具活动状态 */
export interface ToolActivity {
  toolUseId: string
  toolName: string
  input: Record<string, unknown>
  intent?: string
  displayName?: string
  result?: string
  isError?: boolean
  done: boolean
  parentToolUseId?: string
  elapsedSeconds?: number
  taskId?: string
  shellId?: string
  isBackground?: boolean
  /** MCP 工具返回的图片附件 */
  imageAttachments?: Array<{ localPath: string; filename: string; mediaType: string }>
}

/** 活动分组（Task 子代理） */
export interface ActivityGroup {
  parent: ToolActivity
  children: ToolActivity[]
}

/** Teammate 状态枚举 */
export type TeammateStatus = 'running' | 'completed' | 'failed' | 'stopped'

/** 单个 teammate 的实时状态（Agent Teams 功能） */
export interface TeammateState {
  taskId: string
  toolUseId?: string
  description: string
  taskType?: string
  index: number
  status: TeammateStatus
  progressDescription?: string
  currentToolName?: string
  currentToolElapsedSeconds?: number
  currentToolUseId?: string
  toolHistory: string[]
  summary?: string
  outputFile?: string
  usage?: TaskUsage
  startedAt: number
  endedAt?: number
}

/** 工具历史最大记录数 */
const MAX_TOOL_HISTORY = 20

/**
 * 将流式状态中未完成的 toolActivities 和 running teammates 标记为终态。
 */
export function finalizeStreamingActivities(
  toolActivities: ToolActivity[],
  teammates: TeammateState[]
): { toolActivities: ToolActivity[]; teammates: TeammateState[] } {
  const hasUnfinishedTools = toolActivities.some((ta) => !ta.done)
  const hasRunningTeammates = teammates.some((tm) => tm.status === 'running')

  return {
    toolActivities: hasUnfinishedTools
      ? toolActivities.map((ta) => (ta.done ? ta : { ...ta, done: true }))
      : toolActivities,
    teammates: hasRunningTeammates
      ? teammates.map((tm) =>
          tm.status === 'running'
            ? { ...tm, status: 'stopped' as const, endedAt: Date.now(), currentToolName: undefined, currentToolElapsedSeconds: undefined, currentToolUseId: undefined }
            : tm
        )
      : teammates,
  }
}

/** Agent 会话的流式状态 */
export interface AgentStreamState {
  running: boolean
  content: string
  reasoning?: string
  toolActivities: ToolActivity[]
  model?: string
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  isCompacting?: boolean
  compactInFlight?: boolean
  startedAt?: number
  retrying?: {
    currentAttempt: number
    maxAttempts: number
    history: RetryAttempt[]
    failed: boolean
  }
  teammates: TeammateState[]
  waitingResume?: boolean
}

/** 从 ToolActivity 派生状态 */
export function getActivityStatus(activity: ToolActivity): ActivityStatus {
  if (activity.isBackground) return 'backgrounded'
  if (!activity.done) return 'running'
  if (activity.isError) return 'error'
  return 'completed'
}

/**
 * 合并同层 TodoWrite 活动
 */
function mergeTodoWrites(activities: ToolActivity[]): ToolActivity[] {
  const todoWrites: ToolActivity[] = []
  const others: ToolActivity[] = []

  for (const a of activities) {
    if (a.toolName === 'TodoWrite') {
      todoWrites.push(a)
    } else {
      others.push(a)
    }
  }

  if (todoWrites.length === 0) return activities

  const latest = todoWrites[todoWrites.length - 1]!
  const allDone = todoWrites.every((t) => t.done)

  const merged: ToolActivity = {
    ...latest,
    done: allDone,
    isError: allDone && todoWrites.some((t) => t.isError),
  }

  return [...others, merged]
}

/**
 * 将扁平活动列表按 parentToolUseId 分组
 */
export function groupActivities(activities: ToolActivity[]): Array<ActivityGroup | ToolActivity> {
  const filtered = activities.filter((a) => {
    if (a.done && Object.keys(a.input).length === 0 && !a.result) return false
    return true
  })
  const processed = mergeTodoWrites(filtered)

  const parentIds = new Set<string>()
  for (const a of processed) {
    if (a.toolName === 'Task' || a.toolName === 'Agent') parentIds.add(a.toolUseId)
  }

  const childrenMap = new Map<string, ToolActivity[]>()
  const topLevel: Array<ActivityGroup | ToolActivity> = []

  for (const a of processed) {
    if (a.parentToolUseId && parentIds.has(a.parentToolUseId)) {
      const children = childrenMap.get(a.parentToolUseId) ?? []
      children.push(a)
      childrenMap.set(a.parentToolUseId, children)
    } else {
      topLevel.push(a)
    }
  }

  return topLevel.map((item) => {
    if ('toolUseId' in item && parentIds.has(item.toolUseId)) {
      const children = childrenMap.get(item.toolUseId) ?? []
      return { parent: item, children: mergeTodoWrites(children) } as ActivityGroup
    }
    return item
  })
}

/** 判断是否为 ActivityGroup */
export function isActivityGroup(item: ActivityGroup | ToolActivity): item is ActivityGroup {
  return 'parent' in item && 'children' in item
}

/** 待自动发送的 Agent 提示 */
export interface AgentPendingPrompt {
  sessionId: string
  message: string
}

// ===== Atoms =====

export const agentSessionsAtom = atom<AgentSessionMeta[]>([])
export const agentWorkspacesAtom = atom<AgentWorkspace[]>([])
export const currentAgentWorkspaceIdAtom = atom<string | null>(null)
export const agentChannelIdAtom = atom<string | null>(null)
export const agentModelIdAtom = atom<string | null>(null)
export const agentChannelIdsAtom = atom<string[]>([])

export const agentSessionChannelMapAtom = atom<Map<string, string>>(new Map())
export const agentSessionModelMapAtom = atom<Map<string, string>>(new Map())
export const currentAgentSessionIdAtom = atom<string | null>(null)
export const currentAgentMessagesAtom = atom<AgentMessage[]>([])
export const agentStreamingStatesAtom = atom<Map<string, AgentStreamState>>(new Map())

export const liveMessagesMapAtom = atom<Map<string, SDKMessage[]>>(new Map())

export const agentPendingPromptAtom = atom<AgentPendingPrompt | null>(null)
export const agentPendingFilesAtom = atom<AgentPendingFile[]>([])
export const workspaceCapabilitiesVersionAtom = atom(0)
export const workspaceFilesVersionAtom = atom(0)

// ===== 侧面板 Atoms =====

export const agentSidePanelOpenMapAtom = atom<Map<string, boolean>>(new Map())

export const currentSessionSidePanelOpenAtom = atom<boolean>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return false
  return get(agentSidePanelOpenMapAtom).get(currentId) ?? true
})

export const agentSessionPathMapAtom = atom<Map<string, string>>(new Map())

export interface FileBrowserAutoReveal {
  sessionId: string
  path: string
  ts: number
}
export const fileBrowserAutoRevealAtom = atom<FileBrowserAutoReveal | null>(null)

export const recentlyModifiedPathsAtom = atom<Map<string, Map<string, number>>>(new Map())
export const RECENTLY_MODIFIED_TTL_MS = 60_000

// ===== 权限系统 Atoms =====

export const agentDefaultPermissionModeAtom = atom<PromaPermissionMode>('auto')
export const agentPermissionModeMapAtom = atom<Map<string, PromaPermissionMode>>(new Map())
export const agentThinkingAtom = atom<ThinkingConfig | undefined>(undefined)
export const agentEffortAtom = atom<AgentEffort | undefined>(undefined)
export const agentMaxBudgetUsdAtom = atom<number | undefined>(undefined)
export const agentMaxTurnsAtom = atom<number | undefined>(undefined)

export const allPendingPermissionRequestsAtom = atom<Map<string, readonly PermissionRequest[]>>(new Map())

type PermissionRequestsUpdate = readonly PermissionRequest[] | ((prev: readonly PermissionRequest[]) => readonly PermissionRequest[])

export const pendingPermissionRequestsAtom = atom(
  (get): readonly PermissionRequest[] => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return []
    return get(allPendingPermissionRequestsAtom).get(currentId) ?? []
  },
  (get, set, update: PermissionRequestsUpdate) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(allPendingPermissionRequestsAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(currentId) ?? []
      const newValue = typeof update === 'function' ? update(current) : update
      if (newValue.length === 0) map.delete(currentId)
      else map.set(currentId, newValue)
      return map
    })
  }
)

export const allPendingAskUserRequestsAtom = atom<Map<string, readonly AskUserRequest[]>>(new Map())

type AskUserRequestsUpdate = readonly AskUserRequest[] | ((prev: readonly AskUserRequest[]) => readonly AskUserRequest[])

export const pendingAskUserRequestsAtom = atom(
  (get): readonly AskUserRequest[] => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return []
    return get(allPendingAskUserRequestsAtom).get(currentId) ?? []
  },
  (get, set, update: AskUserRequestsUpdate) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(allPendingAskUserRequestsAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(currentId) ?? []
      const newValue = typeof update === 'function' ? update(current) : update
      if (newValue.length === 0) map.delete(currentId)
      else map.set(currentId, newValue)
      return map
    })
  }
)

export const allPendingExitPlanRequestsAtom = atom<Map<string, readonly ExitPlanModeRequest[]>>(new Map())
export const agentPlanModeSessionsAtom = atom<Set<string>>(new Set<string>())

export const currentAgentSessionAtom = atom<AgentSessionMeta | null>((get) => {
  const sessions = get(agentSessionsAtom)
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return sessions.find((s) => s.id === currentId) ?? null
})

export const agentStreamingAtom = atom<boolean>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return false
  return get(agentStreamingStatesAtom).get(currentId)?.running ?? false
})

export const agentStreamingContentAtom = atom<string>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return ''
  return get(agentStreamingStatesAtom).get(currentId)?.content ?? ''
})

export const agentToolActivitiesAtom = atom<ToolActivity[]>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return []
  return get(agentStreamingStatesAtom).get(currentId)?.toolActivities ?? []
})

export const agentStreamingModelAtom = atom<string | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.model
})

export const agentRetryingAtom = atom<AgentStreamState['retrying'] | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.retrying
})

export const agentStartedAtAtom = atom<number | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.startedAt
})

export const agentRunningSessionIdsAtom = atom<Set<string>>((get) => {
  const states = get(agentStreamingStatesAtom)
  const ids = new Set<string>()
  for (const [id, state] of states) {
    if (state.running) ids.add(id)
  }
  return ids
})

export type SessionIndicatorStatus = 'idle' | 'running' | 'blocked' | 'completed'

export const unviewedCompletedSessionIdsAtom = atom<Set<string>>(new Set<string>())
export const workingDoneSessionIdsAtom = atom<Set<string>>(new Set<string>())

export const agentSessionIndicatorMapAtom = atom<Map<string, SessionIndicatorStatus>>((get) => {
  const streamStates = get(agentStreamingStatesAtom)
  const pendingPerms = get(allPendingPermissionRequestsAtom)
  const pendingAskUser = get(allPendingAskUserRequestsAtom)
  const pendingExitPlan = get(allPendingExitPlanRequestsAtom)
  const unviewedCompleted = get(unviewedCompletedSessionIdsAtom)

  const map = new Map<string, SessionIndicatorStatus>()

  for (const [id, state] of streamStates) {
    if (!state.running) continue
    const hasBlock = (pendingPerms.get(id)?.length ?? 0) > 0
      || (pendingAskUser.get(id)?.length ?? 0) > 0
      || (pendingExitPlan.get(id)?.length ?? 0) > 0
    map.set(id, hasBlock ? 'blocked' : 'running')
  }

  for (const id of unviewedCompleted) {
    if (!map.has(id)) {
      map.set(id, 'completed')
    }
  }

  return map
})

function appendToolHistory(history: string[], toolName: string): string[] {
  if (history[history.length - 1] === toolName) return history
  const next = [...history, toolName]
  return next.length > MAX_TOOL_HISTORY ? next.slice(next.length - MAX_TOOL_HISTORY) : next
}

/**
 * 处理 AgentEvent 并更新流式状态（纯函数）
 */
export function applyAgentEvent(
  prev: AgentStreamState,
  event: AgentEvent,
): AgentStreamState {
  switch (event.type) {
    case 'text_delta':
      return { ...prev, content: prev.content + event.text, retrying: undefined }

    case 'text_complete':
      return { ...prev, content: event.text! }

    case 'tool_start': {
      const existing = prev.toolActivities.find((t) => t.toolUseId === event.toolUseId)
      if (existing) {
        return {
          ...prev,
          toolActivities: prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, input: event.input, intent: event.intent || t.intent, displayName: event.displayName || t.displayName }
              : t
          ),
          retrying: undefined,
        }
      }
      return {
        ...prev,
        toolActivities: [...prev.toolActivities, {
          toolUseId: event.toolUseId!,
          toolName: event.toolName!,
          input: event.input,
          intent: event.intent,
          displayName: event.displayName,
          done: false,
          parentToolUseId: event.parentToolUseId,
        }],
        retrying: undefined,
      }
    }

    case 'tool_result':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, result: event.result, isError: event.isError, done: true, imageAttachments: event.imageAttachments }
            : t
        ),
      }

    case 'task_backgrounded':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, isBackground: true, taskId: event.taskId, done: true }
            : t
        ),
      }

    case 'task_progress':
      if (event.taskId) {
        const tmIdx = prev.teammates.findIndex((t) => t.taskId === event.taskId)
        if (tmIdx >= 0) {
          const tm = prev.teammates[tmIdx]!
          const updatedTm: TeammateState = {
            ...tm,
            progressDescription: event.description ?? tm.progressDescription,
            usage: event.usage ?? tm.usage,
            ...(event.lastToolName && {
              currentToolName: event.lastToolName,
              currentToolElapsedSeconds: event.elapsedSeconds ?? tm.currentToolElapsedSeconds,
              currentToolUseId: event.toolUseId,
              toolHistory: appendToolHistory(tm.toolHistory, event.lastToolName),
            }),
            ...(!event.lastToolName && event.elapsedSeconds != null && {
              currentToolElapsedSeconds: event.elapsedSeconds,
            }),
            ...(prev.running && (tm.status === 'stopped' || tm.status === 'failed')
              ? { status: 'running' as const, endedAt: undefined }
              : {}),
          }
          const nextTeammates = [...prev.teammates]
          nextTeammates[tmIdx] = updatedTm
          return { ...prev, teammates: nextTeammates }
        }
      }
      if (event.elapsedSeconds != null) {
        return {
          ...prev,
          toolActivities: prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, elapsedSeconds: event.elapsedSeconds! }
              : t
          ),
        }
      }
      return prev

    case 'task_started': {
      let nextActivities = prev.toolActivities
      if (event.toolUseId) {
        const idx = prev.toolActivities.findIndex((t) => t.toolUseId === event.toolUseId)
        if (idx >= 0) {
          nextActivities = prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, intent: event.description, taskId: event.taskId }
              : t
          )
        }
      }
      if (prev.teammates.some((t) => t.taskId === event.taskId)) {
        return { ...prev, toolActivities: nextActivities }
      }
      const newTeammate: TeammateState = {
        taskId: event.taskId!,
        toolUseId: event.toolUseId,
        description: event.description!,
        taskType: event.taskType,
        index: prev.teammates.length + 1,
        status: 'running',
        toolHistory: [],
        startedAt: Date.now(),
      }
      return {
        ...prev,
        toolActivities: nextActivities,
        teammates: [...prev.teammates, newTeammate],
      }
    }

    case 'shell_backgrounded':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, isBackground: true, shellId: event.shellId, done: true }
            : t
        ),
      }

    case 'shell_killed':
      return prev

    case 'task_notification': {
      const nextTeammates = [...prev.teammates]
      let tmIdx = nextTeammates.findIndex((t) => t.taskId === event.taskId)
      if (tmIdx < 0) {
        nextTeammates.push({
          taskId: event.taskId!,
          toolUseId: event.toolUseId,
          description: event.summary || event.taskId!,
          index: nextTeammates.length + 1,
          status: 'running',
          toolHistory: [],
          startedAt: Date.now(),
        })
        tmIdx = nextTeammates.length - 1
      }
      nextTeammates[tmIdx] = {
        ...nextTeammates[tmIdx]!,
        status: event.status,
        summary: event.summary,
        outputFile: event.outputFile,
        endedAt: Date.now(),
        ...(event.usage && { usage: event.usage }),
        currentToolName: undefined,
        currentToolElapsedSeconds: undefined,
        currentToolUseId: undefined,
      }
      return { ...prev, teammates: nextTeammates }
    }

    case 'tool_use_summary':
      return prev

    case 'waiting_resume':
      return { ...prev, waitingResume: true }

    case 'resume_start':
      return { ...prev, waitingResume: false }

    case 'complete':
      return {
        ...prev,
        retrying: undefined,
        ...finalizeStreamingActivities(prev.toolActivities, prev.teammates),
      }

    case 'typed_error':
      return { ...prev, running: false, retrying: undefined }

    case 'error':
      return { ...prev, running: false }

    case 'usage_update':
      return {
        ...prev,
        inputTokens: event.usage.inputTokens,
        ...(event.usage.outputTokens != null && { outputTokens: event.usage.outputTokens }),
        ...(event.usage.cacheReadTokens != null && { cacheReadTokens: event.usage.cacheReadTokens }),
        ...(event.usage.cacheCreationTokens != null && { cacheCreationTokens: event.usage.cacheCreationTokens }),
        ...(event.usage.costUsd != null && { costUsd: event.usage.costUsd }),
        ...(event.usage.contextWindow && { contextWindow: event.usage.contextWindow }),
      }

    case 'compacting':
      return { ...prev, isCompacting: true, compactInFlight: true }

    case 'compact_complete':
      return { ...prev, isCompacting: false }

    case 'model_resolved':
      return prev

    case 'retrying':
      return {
        ...prev,
        retrying: prev.retrying ?? {
          currentAttempt: event.attempt!,
          maxAttempts: event.maxAttempts!,
          history: [],
          failed: false,
        },
      }

    case 'retry_attempt': {
      const currentHistory = prev.retrying?.history ?? []
      return {
        ...prev,
        retrying: {
          currentAttempt: event.attemptData!.attempt,
          maxAttempts: prev.retrying?.maxAttempts ?? 3,
          history: [...currentHistory, event.attemptData!],
          failed: false,
        },
      }
    }

    case 'retry_cleared':
      return { ...prev, retrying: undefined }

    case 'retry_failed': {
      const finalHistory = prev.retrying?.history ?? []
      return {
        ...prev,
        running: false,
        retrying: {
          currentAttempt: event.finalAttempt!.attempt,
          maxAttempts: prev.retrying?.maxAttempts ?? 3,
          history: [...finalHistory, event.finalAttempt!],
          failed: true,
        },
      }
    }

    case 'permission_request':
    case 'permission_resolved':
    case 'ask_user_request':
    case 'ask_user_resolved':
    case 'prompt_suggestion':
      return prev

    default:
      return prev
  }
}

/** 上下文使用量状态 */
export interface AgentContextStatus {
  isCompacting: boolean
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
}

export const agentContextStatusAtom = atom<AgentContextStatus>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return { isCompacting: false }
  const state = get(agentStreamingStatesAtom).get(currentId)
  return {
    isCompacting: state?.isCompacting ?? false,
    inputTokens: state?.inputTokens,
    outputTokens: state?.outputTokens,
    cacheReadTokens: state?.cacheReadTokens,
    cacheCreationTokens: state?.cacheCreationTokens,
    costUsd: state?.costUsd,
    contextWindow: state?.contextWindow,
  }
})

export const agentStreamErrorsAtom = atom<Map<string, string>>(new Map())
export const agentMessageRefreshAtom = atom<Map<string, number>>(new Map())

export const currentAgentErrorAtom = atom<string | null>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return get(agentStreamErrorsAtom).get(currentId) ?? null
})

export const agentSessionDraftsAtom = atom<Map<string, string>>(new Map())
export const agentSessionDraftHtmlAtom = atom<Map<string, string>>(new Map())
export const agentAttachedDirectoriesMapAtom = atom<Map<string, string[]>>(new Map())
export const workspaceAttachedDirectoriesMapAtom = atom<Map<string, string[]>>(new Map())

export const currentAgentSessionDraftAtom = atom(
  (get) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return ''
    return get(agentSessionDraftsAtom).get(currentId) ?? ''
  },
  (get, set, newDraft: string) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(agentSessionDraftsAtom, (prev) => {
      const map = new Map(prev)
      if (newDraft.trim() === '') {
        map.delete(currentId)
      } else {
        map.set(currentId, newDraft)
      }
      return map
    })
  }
)

// ===== 提示建议 Atoms =====

export const agentPromptSuggestionsAtom = atom<Map<string, string>>(new Map())

export const currentAgentSuggestionAtom = atom<string | null>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return get(agentPromptSuggestionsAtom).get(currentId) ?? null
})

// ===== 后台任务管理 =====

export interface BackgroundTask {
  id: string
  type: 'agent' | 'shell'
  toolUseId: string
  startTime: number
  elapsedSeconds: number
  intent?: string
}

export const backgroundTasksAtomFamily = atomFamily((sessionId: string) =>
  atom<BackgroundTask[]>([])
)

// ===== 用户打断状态 =====

export const stoppedByUserSessionsAtom = atom<Set<string>>(new Set<string>())

// ===== 初始化就绪状态 =====

export const agentSettingsReadyAtom = atom(false)

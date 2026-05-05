/**
 * useGlobalAgentListeners — Agent 全局 IPC 监听
 *
 * 在 App 顶层挂载，监听 Agent 相关的 Tauri 事件流，
 * 将事件分发到对应的 Jotai atom 中。
 *
 * 从 Proma 迁移，IPC 由 tauri-bridge 提供。
 */

import { useEffect, useRef } from 'react'
import { useSetAtom, useAtomValue } from 'jotai'
import {
  agentStreamingStatesAtom,
  applyAgentEvent,
  unviewedCompletedSessionIdsAtom,
  workingDoneSessionIdsAtom,
  agentStreamErrorsAtom,
  allPendingPermissionRequestsAtom,
  allPendingAskUserRequestsAtom,
  allPendingExitPlanRequestsAtom,
  agentPlanModeSessionsAtom,
  currentAgentSessionIdAtom,
  agentPromptSuggestionsAtom,
  stoppedByUserSessionsAtom,
  type AgentStreamState,
} from '@/atoms/agent-atoms'
import type {
  AgentStreamEvent,
  AgentStreamCompletePayload,
  PermissionRequest,
  AskUserRequest,
  ExitPlanModeRequest,
} from '@/lib/proma-types'

/** 创建一个空的初始流式状态 */
function createInitialStreamState(): AgentStreamState {
  return {
    running: true,
    content: '',
    toolActivities: [],
    teammates: [],
    startedAt: Date.now(),
  }
}

export function useGlobalAgentListeners(): void {
  const setStreamStates = useSetAtom(agentStreamingStatesAtom)
  const setUnviewedCompleted = useSetAtom(unviewedCompletedSessionIdsAtom)
  const setWorkingDone = useSetAtom(workingDoneSessionIdsAtom)
  const setStreamErrors = useSetAtom(agentStreamErrorsAtom)
  const setAllPendingPerms = useSetAtom(allPendingPermissionRequestsAtom)
  const setAllPendingAskUser = useSetAtom(allPendingAskUserRequestsAtom)
  const setAllPendingExitPlan = useSetAtom(allPendingExitPlanRequestsAtom)
  const setPlanModeSessions = useSetAtom(agentPlanModeSessionsAtom)
  const setPromptSuggestions = useSetAtom(agentPromptSuggestionsAtom)
  const setStoppedByUser = useSetAtom(stoppedByUserSessionsAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)

  useEffect(() => {
    const api = (window as any).electronAPI
    if (!api) return

    const unsubscribers: Array<() => void> = []

    // Agent 流式事件
    const handleStreamEvent = (payload: AgentStreamEvent) => {
      const { sessionId, event } = payload
      setStreamStates((prev) => {
        const existing = prev.get(sessionId) ?? createInitialStreamState()
        const updated = applyAgentEvent(existing, event)
        const next = new Map(prev)
        next.set(sessionId, updated)
        return next
      })
    }

    // Agent 流式开始
    const handleStreamStart = (payload: { sessionId: string }) => {
      const { sessionId } = payload
      setStreamStates((prev) => {
        const next = new Map(prev)
        next.set(sessionId, createInitialStreamState())
        return next
      })
      // 清除之前的错误
      setStreamErrors((prev) => {
        const next = new Map(prev)
        next.delete(sessionId)
        return next
      })
      // 清除停止标记
      setStoppedByUser((prev) => {
        if (!prev.has(sessionId)) return prev
        const next = new Set(prev)
        next.delete(sessionId)
        return next
      })
    }

    // Agent 流式完成
    const handleStreamComplete = (payload: AgentStreamCompletePayload) => {
      const { sessionId } = payload
      setStreamStates((prev) => {
        const existing = prev.get(sessionId)
        if (!existing) return prev
        const next = new Map(prev)
        next.set(sessionId, { ...existing, running: false })
        return next
      })
      // 如果不是当前查看的会话，标记为未查看完成
      if (sessionId !== currentSessionId) {
        setUnviewedCompleted((prev) => {
          const next = new Set(prev)
          next.add(sessionId)
          return next
        })
      }
      // 标记为 working done
      setWorkingDone((prev) => {
        const next = new Set(prev)
        next.add(sessionId)
        return next
      })
    }

    // Agent 流式错误
    const handleStreamError = (payload: { sessionId: string; error: string }) => {
      const { sessionId, error } = payload
      setStreamErrors((prev) => {
        const next = new Map(prev)
        next.set(sessionId, error)
        return next
      })
      setStreamStates((prev) => {
        const existing = prev.get(sessionId)
        if (!existing) return prev
        const next = new Map(prev)
        next.set(sessionId, { ...existing, running: false })
        return next
      })
    }

    // 权限请求
    const handlePermissionRequest = (payload: { sessionId: string; request: PermissionRequest }) => {
      setAllPendingPerms((prev) => {
        const next = new Map(prev)
        const existing = next.get(payload.sessionId) ?? []
        next.set(payload.sessionId, [...existing, payload.request])
        return next
      })
    }

    // 权限已解决
    const handlePermissionResolved = (payload: { sessionId: string; requestId: string }) => {
      setAllPendingPerms((prev) => {
        const next = new Map(prev)
        const existing = next.get(payload.sessionId)
        if (!existing) return prev
        const filtered = existing.filter((r) => (r as any).id !== payload.requestId)
        if (filtered.length === 0) next.delete(payload.sessionId)
        else next.set(payload.sessionId, filtered)
        return next
      })
    }

    // 监听 electronAPI 兼容层事件
    if (api.onAgentStreamEvent) {
      unsubscribers.push(api.onAgentStreamEvent(handleStreamEvent))
    }
    if (api.onAgentStreamStart) {
      unsubscribers.push(api.onAgentStreamStart(handleStreamStart))
    }
    if (api.onAgentStreamComplete) {
      unsubscribers.push(api.onAgentStreamComplete(handleStreamComplete))
    }
    if (api.onAgentStreamError) {
      unsubscribers.push(api.onAgentStreamError(handleStreamError))
    }
    if (api.onAgentPermissionRequest) {
      unsubscribers.push(api.onAgentPermissionRequest(handlePermissionRequest))
    }
    if (api.onAgentPermissionResolved) {
      unsubscribers.push(api.onAgentPermissionResolved(handlePermissionResolved))
    }

    return () => {
      for (const unsub of unsubscribers) {
        if (typeof unsub === 'function') unsub()
      }
    }
  }, [
    setStreamStates,
    setUnviewedCompleted,
    setWorkingDone,
    setStreamErrors,
    setAllPendingPerms,
    setAllPendingAskUser,
    setAllPendingExitPlan,
    setPlanModeSessions,
    setPromptSuggestions,
    setStoppedByUser,
    currentSessionId,
  ])
}

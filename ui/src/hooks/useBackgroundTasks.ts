/**
 * useBackgroundTasks — 后台任务管理
 *
 * 提供后台任务（backgrounded shell / agent 任务）的查询与更新能力。
 * 从 Proma 迁移，IPC 由 tauri-bridge 提供。
 */

import { useCallback, useMemo } from 'react'
import { useAtomValue } from 'jotai'
import {
  backgroundTasksAtomFamily,
  currentAgentSessionIdAtom,
  type BackgroundTask,
} from '@/atoms/agent-atoms'

export interface UseBackgroundTasksReturn {
  /** 当前会话的后台任务列表 */
  tasks: BackgroundTask[]
  /** 后台任务数量 */
  count: number
  /** 是否有后台任务 */
  hasAny: boolean
}

/**
 * 获取当前 Agent 会话的后台任务。
 * 如果传入 sessionId 则使用指定会话，否则使用当前活动会话。
 */
export function useBackgroundTasks(sessionId?: string): UseBackgroundTasksReturn {
  const currentId = useAtomValue(currentAgentSessionIdAtom)
  const resolvedId = sessionId ?? currentId ?? ''
  const tasks = useAtomValue(backgroundTasksAtomFamily(resolvedId))

  return useMemo(
    () => ({
      tasks,
      count: tasks.length,
      hasAny: tasks.length > 0,
    }),
    [tasks],
  )
}

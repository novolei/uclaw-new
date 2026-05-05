/**
 * useCreateSession — 创建新会话
 *
 * 提供创建 Chat / Agent 会话并自动打开标签页的能力。
 * 创建的会话初始为"草稿"状态，直到发送第一条消息后才在侧边栏显示。
 *
 * 从 Proma 迁移，IPC 由 tauri-bridge 提供。
 */

import { useCallback } from 'react'
import { useSetAtom, useAtomValue } from 'jotai'
import { draftSessionIdsAtom } from '@/atoms/draft-session-atoms'
import { conversationsAtom } from '@/atoms/chat-atoms'
import {
  agentSessionsAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'
import type { TabType } from '@/atoms/tab-atoms'
import { useOpenSession } from './useOpenSession'
import { createConversation, createAgentSession } from '@/lib/tauri-bridge'

export interface CreateSessionOptions {
  type: TabType
  title?: string
  channelId?: string
  workspaceId?: string
}

export type CreateSessionFn = (options?: CreateSessionOptions) => Promise<string>

export function useCreateSession(): CreateSessionFn {
  const openSession = useOpenSession()
  const setDraftIds = useSetAtom(draftSessionIdsAtom)
  const setConversations = useSetAtom(conversationsAtom)
  const setAgentSessions = useSetAtom(agentSessionsAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)

  return useCallback(
    async (options?: CreateSessionOptions) => {
      const type = options?.type ?? 'agent'
      const title = options?.title ?? (type === 'chat' ? '新对话' : '新会话')
      const workspaceId = options?.workspaceId ?? currentWorkspaceId ?? undefined

      // 生成临时 ID（后续后端会返回真实 ID）
      const sessionId = crypto.randomUUID()
      const now = Date.now()

      if (type === 'chat') {
        // 通过 electronAPI 兼容层创建
        try {
          const result = await createConversation({ title } as any)
          const id = result?.id ?? sessionId

          setConversations((prev) => [
            {
              id,
              title,
              createdAt: now,
              updatedAt: now,
              messageCount: 0,
            },
            ...prev,
          ])

          // 标记为草稿
          setDraftIds((prev) => {
            const next = new Set(prev)
            next.add(id)
            return next
          })

          openSession('chat', id, title)
          return id
        } catch (error) {
          console.error('[创建Chat会话失败]:', error)
          throw error
        }
      } else {
        // Agent session
        try {
          const result = await createAgentSession(
            title,
            options?.channelId,
            workspaceId,
          )
          const id = result?.id ?? sessionId

          setAgentSessions((prev) => [
            {
              id,
              title,
              workspaceId,
              channelId: options?.channelId,
              messageCount: 0,
              createdAt: now,
              updatedAt: now,
            },
            ...prev,
          ])

          // 标记为草稿
          setDraftIds((prev) => {
            const next = new Set(prev)
            next.add(id)
            return next
          })

          openSession('agent', id, title)
          return id
        } catch (error) {
          console.error('[创建Agent会话失败]:', error)
          throw error
        }
      }
    },
    [openSession, setDraftIds, setConversations, setAgentSessions, currentWorkspaceId],
  )
}

/**
 * useGlobalChatListeners — 全局 Chat IPC 监听器
 *
 * 在应用顶层挂载，永不销毁。将所有 Chat 流式事件
 * 写入对应 Jotai atoms，确保页面切换时不丢失事件。
 */

import { useEffect } from 'react'
import { useStore } from 'jotai'
import {
  streamingStatesAtom,
  chatStreamErrorsAtom,
  conversationsAtom,
  chatMessageRefreshAtom,
  pendingAgentRecommendationAtom,
} from '@/atoms/chat-atoms'
import { tabsAtom, updateTabTitle } from '@/atoms/tab-atoms'
import type { ConversationStreamState } from '@/atoms/chat-atoms'
import {
  onStreamChunk,
  onStreamReasoning,
  onStreamComplete,
  onStreamError,
  onStreamToolActivity,
  listConversations as listConversationsIPC,
  generateTitle,
  updateConversationTitle,
} from '@/lib/tauri-bridge'

// [PLACEHOLDER] 流式事件类型（后续由 tauri-bridge 提供具体类型）
interface StreamChunkEvent { conversationId: string; delta: string }
interface StreamReasoningEvent { conversationId: string; delta: string }
interface StreamCompleteEvent { conversationId: string }
interface StreamErrorEvent { conversationId: string; error: string }
interface StreamToolActivityEvent { conversationId: string; activity: any }

/** 标题生成输入 */
export interface GenerateTitleInput {
  userMessage: string
  channelId: string
  modelId: string
}

/** 待生成标题的队列（按 conversationId 跟踪） */
const pendingTitles = new Map<string, GenerateTitleInput>()

/**
 * 注册待生成标题信息（由 ChatView.handleSend 在首条消息时调用）
 */
export function registerPendingTitle(conversationId: string, input: GenerateTitleInput): void {
  pendingTitles.set(conversationId, input)
}

export function useGlobalChatListeners(): void {
  const store = useStore()

  useEffect(() => {
    /** 辅助函数：更新 Map 中某个对话的流式状态 */
    const updateState = (
      convId: string,
      updater: (prev: ConversationStreamState) => ConversationStreamState
    ): void => {
      store.set(streamingStatesAtom, (prev) => {
        const current = prev.get(convId) ?? {
          streaming: false,
          content: '',
          reasoning: '',
          model: undefined,
          toolActivities: [],
          startedAt: Date.now(),
        }
        const next = updater(current)
        const map = new Map(prev)
        map.set(convId, next)
        return map
      })
    }

    // ===== 1. 流式内容块 =====
    const cleanupChunk = onStreamChunk(
      (event: StreamChunkEvent) => {
        updateState(event.conversationId, (s) => ({
          ...s,
          content: s.content + event.delta,
        }))
      }
    )

    // ===== 2. 流式推理内容 =====
    const cleanupReasoning = onStreamReasoning(
      (event: StreamReasoningEvent) => {
        updateState(event.conversationId, (s) => ({
          ...s,
          reasoning: s.reasoning + event.delta,
        }))
      }
    )

    // ===== 3. 流式完成 =====
    const cleanupComplete = onStreamComplete(
      (event: StreamCompleteEvent) => {
        updateState(event.conversationId, (s) => ({ ...s, streaming: false }))

        // 递增消息刷新版本号
        store.set(chatMessageRefreshAtom, (prev) => {
          const map = new Map(prev)
          map.set(event.conversationId, (prev.get(event.conversationId) ?? 0) + 1)
          return map
        })

        // 刷新对话列表
        listConversationsIPC()
          .then((convs: any) => store.set(conversationsAtom, convs))
          .catch(console.error)

        // 生成对话标题
        const titleInput = pendingTitles.get(event.conversationId)
        if (titleInput) {
          pendingTitles.delete(event.conversationId)
          generateTitle(titleInput).then((title: string) => {
            if (!title) return
            updateConversationTitle(event.conversationId, title)
              .then((updated: any) => {
                store.set(conversationsAtom, (prev: any[]) =>
                  prev.map((c: any) => (c.id === updated.id ? updated : c))
                )
                store.set(tabsAtom, (prev) => updateTabTitle(prev, event.conversationId, title))
              })
              .catch(console.error)
          }).catch(console.error)
        }
      }
    )

    // ===== 4. 流式错误 =====
    const cleanupError = onStreamError(
      (event: StreamErrorEvent) => {
        updateState(event.conversationId, (s) => ({ ...s, streaming: false }))

        store.set(chatStreamErrorsAtom, (prev) => {
          const map = new Map(prev)
          map.set(event.conversationId, event.error)
          return map
        })

        store.set(chatMessageRefreshAtom, (prev) => {
          const map = new Map(prev)
          map.set(event.conversationId, (prev.get(event.conversationId) ?? 0) + 1)
          return map
        })
      }
    )

    // ===== 5. 工具活动 =====
    const cleanupToolActivity = onStreamToolActivity(
      (event: StreamToolActivityEvent) => {
        updateState(event.conversationId, (s) => ({
          ...s,
          toolActivities: [...s.toolActivities, event.activity],
        }))

        if (
          event.activity.type === 'result'
          && event.activity.toolName === 'suggest_agent_mode'
          && event.activity.result
          && !event.activity.isError
        ) {
          try {
            const parsed = JSON.parse(event.activity.result)
            if (parsed.type === 'agent_recommendation' && parsed.reason && parsed.suggestedPrompt) {
              store.set(pendingAgentRecommendationAtom, {
                reason: parsed.reason,
                suggestedPrompt: parsed.suggestedPrompt,
                conversationId: event.conversationId,
              })
            }
          } catch {
            // ignore
          }
        }
      }
    )

    return () => {
      cleanupChunk()
      cleanupReasoning()
      cleanupComplete()
      cleanupError()
      cleanupToolActivity()
    }
  }, [store])
}

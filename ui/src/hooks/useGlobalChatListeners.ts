/**
 * useGlobalChatListeners — 全局 Chat IPC 监听器
 *
 * 在应用顶层挂载，永不销毁。将所有 Chat 流式事件
 * 写入对应 Jotai atoms，确保页面切换时不丢失事件。
 */

import { useEffect } from 'react'
import { useStore } from 'jotai'

/** Jotai 没有公开导出 Store 类型，这里通过 useStore 的返回类型推导 */
type Store = ReturnType<typeof useStore>
import {
  streamingStatesAtom,
  chatStreamErrorsAtom,
  conversationsAtom,
  chatMessageRefreshAtom,
  pendingAgentRecommendationAtom,
} from '@/atoms/chat-atoms'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
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

export function registerPendingTitle(conversationId: string, input: GenerateTitleInput): void {
  pendingTitles.set(conversationId, input)
}

// ─── Module-level singleton ───────────────────────────────────────────────────

let chatCleanupFns: Array<() => void> = []
let chatInitialized = false

function startChatListeners(store: Store): void {
  if (chatInitialized) return
  chatInitialized = true

  const updateState = (
    convId: string,
    updater: (prev: ConversationStreamState) => ConversationStreamState,
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
      const map = new Map(prev)
      map.set(convId, updater(current))
      return map
    })
  }

  // ===== 1. 流式内容块 =====
  chatCleanupFns.push(
    onStreamChunk((event: StreamChunkEvent) => {
      updateState(event.conversationId, (s) => ({ ...s, content: s.content + event.delta }))
    })
  )

  // ===== 2. 流式推理内容 =====
  chatCleanupFns.push(
    onStreamReasoning((event: StreamReasoningEvent) => {
      updateState(event.conversationId, (s) => ({ ...s, reasoning: s.reasoning + event.delta }))
    })
  )

  // ===== 3. 流式完成 =====
  chatCleanupFns.push(
    onStreamComplete((event: StreamCompleteEvent) => {
      updateState(event.conversationId, (s) => ({ ...s, streaming: false }))

      store.set(chatMessageRefreshAtom, (prev) => {
        const map = new Map(prev)
        map.set(event.conversationId, (prev.get(event.conversationId) ?? 0) + 1)
        return map
      })

      listConversationsIPC()
        .then((convs: any) => store.set(conversationsAtom, convs))
        .catch(console.error)

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
    })
  )

  // ===== 4. 流式错误 =====
  chatCleanupFns.push(
    onStreamError((event: StreamErrorEvent) => {
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
    })
  )

  // ===== 5. 工具活动 =====
  chatCleanupFns.push(
    onStreamToolActivity((event: StreamToolActivityEvent) => {
      // Agent tool activities are handled exclusively by useGlobalAgentListeners.
      const agentSessions = store.get(agentSessionsAtom)
      if (agentSessions.some((s) => s.id === event.conversationId)) return

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
    })
  )

  // Vite HMR: tear down before this module is hot-replaced
  if (import.meta.hot) {
    import.meta.hot.dispose(() => {
      chatInitialized = false
      for (const fn of chatCleanupFns) fn()
      chatCleanupFns = []
    })
  }
}

// ─── React hook ──────────────────────────────────────────────────────────────

export function useGlobalChatListeners(): void {
  const store = useStore()

  useEffect(() => {
    startChatListeners(store)
    // No cleanup — listeners are intentionally global for the app lifetime.
  }, [store])
}

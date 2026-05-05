/**
 * Chat Atoms - 对话相关的 Jotai 状态
 *
 * 管理对话列表、当前对话、消息、流式状态、模型选择、
 * 上下文管理、并排模式、思考模式等。
 * 从 Proma 迁移，类型引用本地化。
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import type {
  ConversationMeta,
  PrimaChatMessage,
  FileAttachment,
  ChatToolActivity,
  Channel,
} from '@/lib/proma-types'

/** 全局渠道列表缓存（启动时加载一次，设置变更时刷新） */
export const channelsAtom = atom<Channel[]>([])

/** 渠道列表是否已完成首次加载 */
export const channelsLoadedAtom = atom(false)

/** 选中的模型信息 */
export interface SelectedModel {
  channelId: string
  modelId: string
}

/** 上下文长度选项值 */
export type ContextLengthValue = 0 | 5 | 10 | 15 | 20 | 'infinite'

/** 上下文长度选项列表 */
export const CONTEXT_LENGTH_OPTIONS: ContextLengthValue[] = [0, 5, 10, 15, 20, 'infinite']

/** 对话列表 */
export const conversationsAtom = atom<ConversationMeta[]>([])

/** 当前对话 ID */
export const currentConversationIdAtom = atom<string | null>(null)

/** 当前对话的消息列表 */
export const currentMessagesAtom = atom<PrimaChatMessage[]>([])

/** 单个对话的流式状态 */
export interface ConversationStreamState {
  streaming: boolean
  content: string
  reasoning: string
  model?: string
  toolActivities: ChatToolActivity[]
  startedAt?: number
}

/**
 * 全局流式状态 Map — 以 conversationId 为 key
 */
export const streamingStatesAtom = atom<Map<string, ConversationStreamState>>(new Map())

/**
 * 当前正在流式输出的对话 ID 集合（派生只读原子）
 */
export const streamingConversationIdsAtom = atom<Set<string>>((get) => {
  const states = get(streamingStatesAtom)
  const ids = new Set<string>()
  for (const [id, state] of states) {
    if (state.streaming) ids.add(id)
  }
  return ids
})

export const streamingAtom = atom<boolean>(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return false
    return get(streamingStatesAtom).get(currentId)?.streaming ?? false
  },
)

export const streamingContentAtom = atom<string>(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return ''
    return get(streamingStatesAtom).get(currentId)?.content ?? ''
  },
)

export const streamingReasoningAtom = atom<string>(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return ''
    return get(streamingStatesAtom).get(currentId)?.reasoning ?? ''
  },
)

export const streamingModelAtom = atom<string | null>(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return null
    return get(streamingStatesAtom).get(currentId)?.model ?? null
  },
)

export const streamingToolActivitiesAtom = atom<ChatToolActivity[]>(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return []
    return get(streamingStatesAtom).get(currentId)?.toolActivities ?? []
  },
)

/** 选中的模型（持久化到 localStorage） */
export const selectedModelAtom = atomWithStorage<SelectedModel | null>(
  'uclaw-selected-model',
  null,
)

/** 当前对话的元数据（派生原子） */
export const currentConversationAtom = atom<ConversationMeta | null>((get) => {
  const conversations = get(conversationsAtom)
  const currentId = get(currentConversationIdAtom)
  if (!currentId) return null
  return conversations.find((c) => c.id === currentId) ?? null
})

/** 上下文长度（持久化到 localStorage，默认不限制） */
export const contextLengthAtom = atomWithStorage<ContextLengthValue>(
  'uclaw-context-length',
  'infinite',
)

/** 并排模式 */
export const parallelModeAtom = atom<boolean>(false)

/** 思考模式（持久化到 localStorage） */
export const thinkingEnabledAtom = atomWithStorage<boolean>(
  'uclaw-thinking-enabled',
  false,
)

/** 当前对话的上下文分隔线 */
export const contextDividersAtom = atom<string[]>([])

/** 待发送的附件（含本地预览 URL） */
export interface PendingAttachment extends FileAttachment {
  previewUrl?: string
}

/** 待发送附件列表 */
export const pendingAttachmentsAtom = atom<PendingAttachment[]>([])

/** 是否还有更多历史消息未加载 */
export const hasMoreMessagesAtom = atom<boolean>(false)

/** 初次加载的消息条数 */
export const INITIAL_MESSAGE_LIMIT = 10

/**
 * 流式错误消息 Map — 以 conversationId 为 key
 */
export const chatStreamErrorsAtom = atom<Map<string, string>>(new Map())

/** 当前对话的错误消息（派生只读原子） */
export const currentChatErrorAtom = atom<string | null>((get) => {
  const currentId = get(currentConversationIdAtom)
  if (!currentId) return null
  return get(chatStreamErrorsAtom).get(currentId) ?? null
})

/**
 * 对话输入框草稿 Map — 以 conversationId 为 key
 */
export const conversationDraftsAtom = atom<Map<string, string>>(new Map())

/** 当前对话的草稿内容（派生读写原子） */
export const currentConversationDraftAtom = atom(
  (get) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return ''
    return get(conversationDraftsAtom).get(currentId) ?? ''
  },
  (get, set, newDraft: string) => {
    const currentId = get(currentConversationIdAtom)
    if (!currentId) return
    set(conversationDraftsAtom, (prev) => {
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

// ===== 快速任务待发送消息 =====

export interface ChatPendingMessage {
  conversationId: string
  message: string
  attachments?: FileAttachment[]
}

export const chatPendingMessageAtom = atom<ChatPendingMessage | null>(null)

export const chatMessageRefreshAtom = atom<Map<string, number>>(new Map())

// ===== Agent 模式推荐 =====

export interface AgentRecommendation {
  reason: string
  suggestedPrompt: string
  conversationId: string
}

export const pendingAgentRecommendationAtom = atom<AgentRecommendation | null>(null)

// ===== Per-conversation 设置 Map =====

export const conversationModelsAtom = atom<Map<string, SelectedModel | null>>(new Map())
export const conversationContextLengthAtom = atom<Map<string, ContextLengthValue>>(new Map())
export const conversationThinkingEnabledAtom = atom<Map<string, boolean>>(new Map())
export const conversationParallelModeAtom = atom<Map<string, boolean>>(new Map())

/** 思考块默认展开偏好（持久化到 localStorage） */
export const thinkingExpandedAtom = atomWithStorage<boolean>(
  'uclaw-thinking-expanded',
  false,
)

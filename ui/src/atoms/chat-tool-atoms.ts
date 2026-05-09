/**
 * Chat Tool Atoms - Chat 工具状态管理
 *
 * 从 Proma 迁移，类型引用本地化。
 */

import { atom } from 'jotai'
import type { ChatToolInfo } from '@/lib/chat-types'

/** 从后端加载的所有工具列表（唯一状态源） */
export const chatToolsAtom = atom<ChatToolInfo[]>([])

/**
 * 派生：当前实际启用的工具 ID 列表
 */
export const activeToolIdsAtom = atom<string[]>((get) => {
  const allTools = get(chatToolsAtom)
  return allTools
    .filter((t) => t.enabled && t.available)
    .map((t) => t.meta.id)
})

/** 派生：是否有任何工具启用 */
export const hasActiveToolsAtom = atom<boolean>((get) => {
  return get(activeToolIdsAtom).length > 0
})

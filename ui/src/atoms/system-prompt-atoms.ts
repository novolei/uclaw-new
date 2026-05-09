/**
 * System Prompt Atoms - 系统提示词状态管理
 *
 * 从 Proma 迁移，类型引用本地化。
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import { BUILTIN_DEFAULT_ID, BUILTIN_DEFAULT_PROMPT } from '@/lib/chat-types'
import type { SystemPromptConfig, SystemPrompt } from '@/lib/chat-types'
import { userProfileAtom } from './user-profile'

/** 提示词编辑侧栏是否打开 */
export const promptSidebarOpenAtom = atom<boolean>(false)

/** 完整提示词配置（从后端加载） */
export const promptConfigAtom = atom<SystemPromptConfig>({
  prompts: [BUILTIN_DEFAULT_PROMPT],
  defaultPromptId: BUILTIN_DEFAULT_ID,
  appendDateTimeAndUserName: true,
})

/** 当前选中的提示词 ID（持久化到 localStorage） */
export const selectedPromptIdAtom = atomWithStorage<string>(
  'uclaw-selected-system-prompt-id',
  BUILTIN_DEFAULT_ID
)

/** 提示词列表（派生只读） */
export const promptListAtom = atom<SystemPrompt[]>(
  (get) => get(promptConfigAtom).prompts
)

/** 默认提示词 ID（派生只读） */
export const defaultPromptIdAtom = atom<string | undefined>(
  (get) => get(promptConfigAtom).defaultPromptId
)

/** 当前选中的提示词对象（派生只读） */
export const selectedPromptAtom = atom<SystemPrompt | undefined>((get) => {
  const config = get(promptConfigAtom)
  const selectedId = get(selectedPromptIdAtom)
  return config.prompts.find((p) => p.id === selectedId)
})

/** 解析最终 systemMessage（派生只读） */
export const resolvedSystemMessageAtom = atom<string | undefined>((get) => {
  const selectedPrompt = get(selectedPromptAtom)
  if (!selectedPrompt) return undefined

  let message = selectedPrompt.content

  const config = get(promptConfigAtom)
  if (config.appendDateTimeAndUserName) {
    const userProfile = get(userProfileAtom)
    const now = new Date()
    const dateTimeStr = now.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      weekday: 'long',
    })
    const appendix = `\n\n---\n当前时间: ${dateTimeStr}\n用户名: ${userProfile.userName}`
    message += appendix
  }

  return message
})

// ===== Per-conversation 系统提示词 =====

export const conversationPromptIdAtom = atom<Map<string, string>>(new Map())

/** 根据 promptId 解析 systemMessage（纯函数） */
export function resolveSystemMessage(
  promptId: string,
  config: SystemPromptConfig,
  userName: string,
): string | undefined {
  const prompt = config.prompts.find((p) => p.id === promptId)
  if (!prompt) return undefined

  let message = prompt.content

  if (config.appendDateTimeAndUserName) {
    const now = new Date()
    const dateTimeStr = now.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      weekday: 'long',
    })
    message += `\n\n---\n当前时间: ${dateTimeStr}\n用户名: ${userName}`
  }

  return message
}

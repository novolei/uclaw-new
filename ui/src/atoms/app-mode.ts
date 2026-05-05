/**
 * App Mode Atom - 应用模式状态
 *
 * - chat: 对话模式
 * - agent: Agent 模式
 */

import { atomWithStorage } from 'jotai/utils'

export type AppMode = 'chat' | 'agent'

/** App 模式，自动持久化到 localStorage */
export const appModeAtom = atomWithStorage<AppMode>('uclaw-app-mode', 'agent')

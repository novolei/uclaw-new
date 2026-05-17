/**
 * App Mode Atom - 应用模式状态
 *
 * - chat:     对话模式
 * - agent:    Agent 模式
 * - symphony: Symphony 模式 (DAG-of-agent-runs orchestration canvas)
 */

import { atomWithStorage } from 'jotai/utils'

export type AppMode = 'chat' | 'agent' | 'symphony'

/** App 模式，自动持久化到 localStorage */
export const appModeAtom = atomWithStorage<AppMode>('uclaw-app-mode', 'agent')

/**
 * 飞书集成 Jotai 状态（多 Bot 版本）
 *
 * 从 Proma 迁移，类型引用本地化。
 * uClaw 暂不支持飞书集成，保留接口以备后续扩展。
 */

import { atom } from 'jotai'
import type { FeishuBridgeState, FeishuNotifyMode, FeishuChatBinding, FeishuBotBridgeState } from '@/lib/chat-types'

/** 多 Bot Bridge 状态（botId → 状态） */
export const feishuBotStatesAtom = atom<Record<string, FeishuBotBridgeState>>({})

/** 任一 Bot 已连接（derived） */
export const feishuAnyConnectedAtom = atom((get) =>
  Object.values(get(feishuBotStatesAtom)).some((b) => b.status === 'connected'),
)

/** 飞书 Bridge 连接状态（向后兼容） */
export const feishuBridgeStateAtom = atom<FeishuBridgeState>((get) => {
  const states = get(feishuBotStatesAtom)
  const first = Object.values(states)[0]
  return first ?? { status: 'disconnected', activeBindings: 0 }
})

/** 全局默认通知模式 */
export const feishuDefaultNotifyModeAtom = atom<FeishuNotifyMode>('auto')

/** per-session 通知模式 */
export const sessionFeishuNotifyModeAtom = atom<Map<string, FeishuNotifyMode>>(new Map())

/** 飞书是否已连接 */
export const feishuConnectedAtom = feishuAnyConnectedAtom

/** 飞书聊天绑定列表 */
export const feishuBindingsAtom = atom<FeishuChatBinding[]>([])

/**
 * 钉钉集成 Jotai 状态（多 Bot 版本）
 *
 * 从 Proma 迁移，类型引用本地化。
 * uClaw 暂不支持钉钉集成，保留接口以备后续扩展。
 */

import { atom } from 'jotai'
import type { DingTalkBridgeState, DingTalkBotBridgeState } from '@/lib/proma-types'

/** 所有 Bot 的状态（botId → 状态） */
export const dingtalkBotStatesAtom = atom<Record<string, DingTalkBotBridgeState>>({})

/** 任一 Bot 已连接 */
export const dingtalkAnyConnectedAtom = atom((get) => {
  const states = get(dingtalkBotStatesAtom)
  return Object.values(states).some((s) => s.status === 'connected')
})

/** 钉钉 Bridge 连接状态（向后兼容） */
export const dingtalkBridgeStateAtom = atom<DingTalkBridgeState>((get) => {
  const states = get(dingtalkBotStatesAtom)
  const first = Object.values(states)[0]
  return first ?? { status: 'disconnected' }
})

/** 钉钉是否已连接 */
export const dingtalkConnectedAtom = dingtalkAnyConnectedAtom

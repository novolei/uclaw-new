/**
 * 微信集成 Jotai 状态
 *
 * 从 Proma 迁移，类型引用本地化。
 * uClaw 暂不支持微信集成，保留接口以备后续扩展。
 */

import { atom } from 'jotai'
import type { WeChatBridgeState } from '@/lib/proma-types'

/** 微信 Bridge 连接状态 */
export const wechatBridgeStateAtom = atom<WeChatBridgeState>({
  status: 'disconnected',
})

/** 微信是否已连接（derived atom） */
export const wechatConnectedAtom = atom((get) => get(wechatBridgeStateAtom).status === 'connected')

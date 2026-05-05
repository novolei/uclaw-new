/**
 * 代理配置状态管理
 *
 * 从 Proma 迁移，占位实现（uClaw 暂不支持代理配置）。
 */

import { atom } from 'jotai'
import type { ProxyConfig } from '@/lib/proma-types'

/** 代理配置 Atom */
export const proxyConfigAtom = atom<ProxyConfig | null>(null)

/** 加载代理配置（占位） */
export const loadProxyConfigAtom = atom(null, async (_get, set) => {
  // TODO: uClaw 后端支持代理配置后实现
  set(proxyConfigAtom, null)
})

/** 更新代理配置（占位） */
export const updateProxyConfigAtom = atom(
  null,
  async (_get, set, config: ProxyConfig) => {
    // TODO: uClaw 后端支持代理配置后实现
    set(proxyConfigAtom, config)
  }
)

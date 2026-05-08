/**
 * activeProviderModelAtom — 全局活跃模型选择
 *
 * 持久化策略：
 * - localStorage ('uclaw-active-provider-model') 作为即时缓存，UI 无感刷新
 * - ~/.uclaw/providers.json (backend) 作为权威持久化存储
 * - 应用启动时从 backend 同步，确保跨设备/重装后一致性
 * - 用户在 composer 选择模型时同时更新两者
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export interface ActiveProviderModel {
  providerId: string
  modelId: string
}

/** 当前全局活跃模型，localStorage 持久化 + 启动时从 backend 同步 */
export const activeProviderModelAtom = atomWithStorage<ActiveProviderModel | null>(
  'uclaw-active-provider-model',
  null,
)

/** 派生: "providerId/modelId" 格式的 ref 字符串，供 role 系统比对 */
export const activeModelRefAtom = atom((get) => {
  const m = get(activeProviderModelAtom)
  return m ? `${m.providerId}/${m.modelId}` : null
})

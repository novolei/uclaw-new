/**
 * Kaleidoscope Atoms — 万花筒内部状态
 *
 *  - kaleidoscopeModuleAtom : 当前选中的模块
 *  - KALEIDOSCOPE_MODULES   : 7 个模块的元数据表（id / 标签 / 分组），
 *                             Rail 的导航顺序与分组分隔以此为准
 *
 * 分组语义：
 *  - 'asset'      资产 —— 你拥有 / 积累的东西（数字人 / 应用商店 / 我的应用 / 产出）
 *  - 'capability' 能力 —— 你 Agent 的内功（技能 / 集成 / 记忆）
 *
 * 不持久化 —— Phase 1 每次进万花筒都从 'humans' 开始。
 */
import { atom } from 'jotai'

export type KaleidoscopeModuleId =
  | 'humans' | 'store' | 'apps' | 'artifacts'
  | 'skills' | 'integrations' | 'memory' | 'evolution'

export type KaleidoscopeGroup = 'asset' | 'capability'

export type BuiltinIntegrationId = 'playwright_mcp'

export interface KaleidoscopeModuleMeta {
  id: KaleidoscopeModuleId
  /** Rail 上显示的中文标签 */
  label: string
  group: KaleidoscopeGroup
}

export const KALEIDOSCOPE_MODULES: KaleidoscopeModuleMeta[] = [
  { id: 'humans', label: '数字人', group: 'asset' },
  { id: 'store', label: '应用商店', group: 'asset' },
  { id: 'apps', label: '我的应用', group: 'asset' },
  { id: 'artifacts', label: '产出', group: 'asset' },
  { id: 'skills', label: '技能', group: 'capability' },
  { id: 'integrations', label: '集成', group: 'capability' },
  { id: 'memory', label: '记忆', group: 'capability' },
  { id: 'evolution', label: '进化', group: 'capability' },
]

export const kaleidoscopeModuleAtom = atom<KaleidoscopeModuleId>('humans')

export const selectedBuiltinIntegrationAtom = atom<BuiltinIntegrationId | null>(null)

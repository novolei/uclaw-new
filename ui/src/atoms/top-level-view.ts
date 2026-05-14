/**
 * Top-Level View Atom — 最高层视图状态
 *
 * uClaw 有两个并行的顶层 surface：
 *  - 'workspace'    : 任务流（chat / agent / 文件 / artifact），由 WorkspaceShell 承载
 *  - 'kaleidoscope' : 配置流（数字人 / 应用 / 技能 / 集成 / 记忆 / 产出），由 KaleidoscopeShell 承载
 *
 * 不持久化 —— 应用重启回到 'workspace'（任务流是默认入口）。
 * `appModeAtom`（chat/agent）只在 topLevelView === 'workspace' 时生效。
 */
import { atom } from 'jotai'

export type TopLevelView = 'workspace' | 'kaleidoscope'

export const topLevelViewAtom = atom<TopLevelView>('workspace')

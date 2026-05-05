/**
 * 环境检测状态管理
 *
 * 从 Proma 迁移，简化实现（uClaw 不需要 Node/Git/Shell 环境检测）。
 */

import { atom } from 'jotai'
import type {
  EnvironmentCheckResult,
  RuntimeStatus,
  InstallerManifest,
} from '@/lib/proma-types'

/** 单个安装包的下载状态 */
export interface InstallerDownloadState {
  status: 'idle' | 'downloading' | 'verifying' | 'done' | 'failed' | 'cancelled'
  downloaded?: number
  total?: number
  speed?: number
  filePath?: string
  error?: string
}

export const environmentCheckResultAtom = atom<EnvironmentCheckResult | null>(null)
export const runtimeStatusAtom = atom<RuntimeStatus | null>(null)
export const isCheckingEnvironmentAtom = atom(false)
export const installerManifestAtom = atom<InstallerManifest | null>(null)
export const installerDownloadStatesAtom = atom<Record<string, InstallerDownloadState>>({})

export const hasEnvironmentIssuesAtom = atom((get) => {
  const result = get(environmentCheckResultAtom)
  if (!result) return false
  return result.hasIssues
})

export const isShellEnvironmentOkAtom = atom((_get) => {
  // uClaw 不依赖 Shell 环境
  return true
})

export const isNodeJsOkAtom = atom((_get) => {
  // uClaw 不依赖 Node.js
  return true
})

export const environmentCheckDialogOpenAtom = atom(false)

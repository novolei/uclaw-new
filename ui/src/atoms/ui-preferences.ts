/**
 * UI 偏好设置状态管理
 *
 * 管理用户界面相关的显示偏好。
 * 从 Proma 迁移，IPC 使用 tauri-bridge 适配层。
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import * as bridge from '@/lib/tauri-bridge'

// ===== localStorage 缓存键 =====

const STICKY_USER_MESSAGE_CACHE_KEY = 'uclaw-sticky-user-message'

// ===== Jotai Atoms =====

/** 是否显示用户消息悬浮置顶条 */
export const stickyUserMessageEnabledAtom = atom<boolean>(true)

/**
 * 是否显示输入框上方的 Agent 状态条（AgentStatusBar）。
 * 默认关闭——流式气泡内已有 AgentRunningIndicator，状态条属可选的额外信息密度。
 * 用 atomWithStorage 自持久化，无需 initializeUiPreferences 接线。
 */
export const agentStatusBarEnabledAtom = atomWithStorage<boolean>(
  'uclaw-agent-status-bar',
  false,
)

/**
 * 是否启用 Plan 模式自动建议横幅（PlanModeSuggestBanner）。
 * 用户点击"不再建议"后置为 false，持久化到 localStorage。
 */
export const planModeSuggestEnabledAtom = atomWithStorage<boolean>(
  'uclaw-plan-mode-suggest-enabled',
  true,
)

// ===== 缓存读取 =====

function getCachedStickyUserMessage(): boolean {
  try {
    const cached = localStorage.getItem(STICKY_USER_MESSAGE_CACHE_KEY)
    if (cached === 'true' || cached === 'false') {
      return cached === 'true'
    }
  } catch {
    // localStorage 不可用时忽略
  }
  return true
}

// ===== 初始化 =====

/**
 * 从后端加载 UI 偏好设置
 */
export async function initializeUiPreferences(
  setStickyUserMessageEnabled: (enabled: boolean) => void
): Promise<void> {
  try {
    // uClaw settings 目前不含此字段，使用缓存值
    const cached = getCachedStickyUserMessage()
    setStickyUserMessageEnabled(cached)
    await bridge.getSettings()
  } catch (error) {
    console.error('[UI偏好] 初始化失败:', error)
  }
}

// ===== 持久化更新 =====

/**
 * 更新悬浮置顶条开关并持久化
 */
export async function updateStickyUserMessageEnabled(enabled: boolean): Promise<void> {
  // 使用 localStorage 缓存用户偏好设置
  try {
    localStorage.setItem(STICKY_USER_MESSAGE_CACHE_KEY, String(enabled))
  } catch {
    // localStorage 不可用时忽略
  }
}

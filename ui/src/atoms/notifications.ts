/**
 * 桌面通知状态管理
 *
 * 管理通知开关状态，提供发送桌面通知的工具函数。
 * 从 Proma 迁移，IPC 使用 tauri-bridge 适配层。
 */

import { atom } from 'jotai'
import type { NotificationSoundId, NotificationSoundType, NotificationSoundSettings } from '@/lib/proma-types'
import * as bridge from '@/lib/tauri-bridge'

// ===== Jotai Atoms =====

/** 通知是否启用 */
export const notificationsEnabledAtom = atom<boolean>(true)

/** 通知提示音是否启用 */
export const notificationSoundEnabledAtom = atom<boolean>(true)

/** 各场景通知音配置 */
export const notificationSoundsAtom = atom<NotificationSoundSettings>({})

/** 各场景的默认通知音 */
export const DEFAULT_NOTIFICATION_SOUNDS: Required<NotificationSoundSettings> = {
  taskComplete: 'ding',
  permissionRequest: 'ding-dong',
  exitPlanMode: 'ding-dong',
}

// ===== 初始化 =====

/**
 * 从后端加载通知设置
 */
export async function initializeNotifications(
  setEnabled: (enabled: boolean) => void,
  setSoundEnabled: (enabled: boolean) => void,
  _setSounds: (sounds: NotificationSoundSettings) => void
): Promise<void> {
  try {
    // uClaw settings 目前不含通知设置字段，保持默认值
    await bridge.getSettings()
    setEnabled(true)
    setSoundEnabled(true)
  } catch (error) {
    console.error('[通知] 初始化失败:', error)
  }
}

// ===== 音频播放（简化版，不含预置音效资源） =====

/**
 * 播放指定通知音（占位，需后续添加音效资源）
 */
export function playNotificationSound(_soundId: NotificationSoundId): void {
  // TODO: 添加音效资源后实现
}

/**
 * 根据场景类型播放对应通知音
 */
export function playNotificationSoundForType(
  type: NotificationSoundType,
  sounds: NotificationSoundSettings
): void {
  const soundId = sounds[type] ?? DEFAULT_NOTIFICATION_SOUNDS[type]
  playNotificationSound(soundId)
}

// ===== 桌面通知 =====

export interface DesktopNotificationOptions {
  soundType?: NotificationSoundType
  playSound?: boolean
  sounds?: NotificationSoundSettings
  onNavigate?: () => void
  force?: boolean
}

/**
 * 发送桌面通知
 */
export function sendDesktopNotification(
  title: string,
  body: string,
  enabled: boolean,
  options?: DesktopNotificationOptions
): void {
  if (options?.playSound && options.soundType) {
    playNotificationSoundForType(options.soundType, options.sounds ?? {})
  }

  if (!enabled) return
  if (!options?.force && document.hasFocus()) return

  const notification = new Notification(title, { body, silent: true })
  notification.onclick = () => {
    window.focus()
    options?.onNavigate?.()
  }
}

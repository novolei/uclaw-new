/**
 * useShortcut — 快捷键绑定 Hook
 *
 * 监听键盘事件，匹配注册的快捷键并触发回调。
 * 支持 Mac / Windows 平台差异。
 *
 * 从 Proma 迁移。
 */

import { useEffect, useCallback, useRef } from 'react'
import { useAtomValue } from 'jotai'
import { shortcutOverridesAtom } from '@/atoms/shortcut-atoms'
import { getShortcutForPlatform, type ShortcutDefinition } from '@/lib/shortcut-defaults'

/**
 * 判断当前是否为 Mac 平台
 */
const isMac = typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

/**
 * 解析快捷键字符串为标准化格式
 * 例如 "Cmd+Shift+K" → { meta: true, shift: true, key: 'k' }
 */
function parseShortcut(shortcut: string): {
  meta: boolean
  ctrl: boolean
  alt: boolean
  shift: boolean
  key: string
} {
  const parts = shortcut.toLowerCase().split('+').map((p) => p.trim())
  return {
    meta: parts.includes('cmd') || parts.includes('meta') || parts.includes('⌘'),
    ctrl: parts.includes('ctrl') || parts.includes('control'),
    alt: parts.includes('alt') || parts.includes('option') || parts.includes('⌥'),
    shift: parts.includes('shift') || parts.includes('⇧'),
    key: parts[parts.length - 1] ?? '',
  }
}

/**
 * 检查键盘事件是否匹配快捷键
 */
function matchesShortcut(
  e: KeyboardEvent,
  shortcut: string,
): boolean {
  const parsed = parseShortcut(shortcut)

  const modMeta = isMac ? e.metaKey : e.ctrlKey
  const modCtrl = isMac ? e.ctrlKey : false

  if (parsed.meta !== modMeta) return false
  if (parsed.ctrl !== modCtrl) return false
  if (parsed.alt !== e.altKey) return false
  if (parsed.shift !== e.shiftKey) return false

  return e.key.toLowerCase() === parsed.key
}

export interface UseShortcutOptions {
  /** 快捷键 ID（对应 shortcut-defaults 中的定义） */
  id: string
  /** 触发时的回调 */
  handler: (e: KeyboardEvent) => void
  /** 是否禁用（默认 false） */
  disabled?: boolean
  /** 是否阻止默认行为（默认 true） */
  preventDefault?: boolean
}

/**
 * 绑定单个快捷键。
 * 自动读取用户覆盖配置，如果有覆盖则使用覆盖值。
 */
export function useShortcut({
  id,
  handler,
  disabled = false,
  preventDefault = true,
}: UseShortcutOptions): void {
  const overrides = useAtomValue(shortcutOverridesAtom)
  const handlerRef = useRef(handler)
  handlerRef.current = handler

  useEffect(() => {
    if (disabled) return

    const override = overrides[id]
    const shortcutStr = override
      ? (isMac ? override.mac : override.win) ?? getShortcutForPlatform(id)
      : getShortcutForPlatform(id)

    if (!shortcutStr) return

    const onKeyDown = (e: KeyboardEvent) => {
      if (matchesShortcut(e, shortcutStr)) {
        if (preventDefault) e.preventDefault()
        handlerRef.current(e)
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [id, disabled, preventDefault, overrides])
}

/**
 * 绑定多个快捷键。
 */
export function useShortcuts(
  shortcuts: Array<Omit<UseShortcutOptions, 'handler'> & { handler: (e: KeyboardEvent) => void }>,
): void {
  const overrides = useAtomValue(shortcutOverridesAtom)
  const handlersRef = useRef(shortcuts)
  handlersRef.current = shortcuts

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      for (const shortcut of handlersRef.current) {
        if (shortcut.disabled) continue

        const override = overrides[shortcut.id]
        const shortcutStr = override
          ? (isMac ? override.mac : override.win) ?? getShortcutForPlatform(shortcut.id)
          : getShortcutForPlatform(shortcut.id)

        if (!shortcutStr) continue

        if (matchesShortcut(e, shortcutStr)) {
          if (shortcut.preventDefault !== false) e.preventDefault()
          shortcut.handler(e)
          return
        }
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [overrides])
}

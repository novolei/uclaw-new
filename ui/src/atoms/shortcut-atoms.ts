/**
 * Shortcut Atoms — 快捷键状态管理
 *
 * 管理用户自定义快捷键覆盖配置。
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import type { ShortcutOverrides } from '@/lib/chat-types'

/**
 * User-defined shortcut overrides.
 *
 * Persisted to localStorage via atomWithStorage so the user's rebindings
 * survive across app restarts. Keyed by the shortcut id (matching
 * `SHORTCUT_DEFINITIONS[].id`); each entry may override `mac`, `win`, or
 * both — missing values fall back to the platform default.
 *
 * Empty `{}` is the "no overrides, every shortcut uses its default" state.
 * Settings UI sets / clears entries here; useShortcut reads it to resolve
 * the effective binding for the current platform.
 */
export const shortcutOverridesAtom = atomWithStorage<ShortcutOverrides>(
  'uclaw-shortcut-overrides',
  {},
)

/** 发送消息快捷键模式：true = Cmd/Ctrl+Enter 发送，false = Enter 发送 */
export const sendWithCmdEnterAtom = atom(false)

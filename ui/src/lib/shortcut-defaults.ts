/**
 * shortcut-defaults — 快捷键默认配置
 *
 * 定义所有快捷键的默认值，区分 Mac / Windows 平台。
 * 从 Proma 迁移。
 */

export interface ShortcutDefinition {
  /** 快捷键 ID（唯一标识） */
  id: string
  /** 人类可读的描述 */
  label: string
  /** 所属分组 */
  group: string
  /** Mac 平台默认快捷键 */
  mac: string
  /** Windows 平台默认快捷键 */
  win: string
}

const isMac = typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

/**
 * 所有快捷键定义
 */
/**
 * Only shortcuts with a real handler are listed below. Earlier this file
 * also contained 9 "ghost" entries (close-tab, next-tab, prev-tab,
 * open-shortcuts, search-conversations, clear-input, stop-generation,
 * toggle-thinking, toggle-side-panel) — each declared a binding but had
 * no `useShortcut({ id: ... })` site anywhere, so pressing the combo did
 * nothing AND exposing them in Settings → 快捷键 misled users into
 * thinking they could be rebound. Removed 2026-05-13. The Esc-stops-
 * generation behaviour is unrelated to this registry; `shortcut-registry`
 * placeholder + custom `proma:stop-generation` window event drive it.
 */
export const SHORTCUT_DEFINITIONS: ShortcutDefinition[] = [
  // ─── 导航 ───
  {
    id: 'new-chat',
    label: '新建对话',
    group: '导航',
    mac: 'Cmd+N',
    win: 'Ctrl+N',
  },
  {
    id: 'new-agent-session',
    label: '新建 Agent 会话',
    group: '导航',
    mac: 'Cmd+Shift+N',
    win: 'Ctrl+Shift+N',
  },
  {
    id: 'open-settings',
    label: '打开设置',
    group: '导航',
    mac: 'Cmd+,',
    win: 'Ctrl+,',
  },

  // ─── 工作区切换 (导航 group) ───
  ...Array.from({ length: 9 }, (_, i) => ({
    id: `switch-workspace-${i + 1}`,
    label: `切换到第 ${i + 1} 个工作区`,
    group: '导航',
    mac: `Cmd+${i + 1}`,
    win: `Ctrl+${i + 1}`,
  })),

  // ─── 搜索 ───
  {
    id: 'global-search',
    label: '全局搜索',
    group: '搜索',
    mac: 'Cmd+K',
    win: 'Ctrl+K',
  },

  // ─── 编辑 ───
  {
    id: 'focus-input',
    label: '聚焦输入框',
    group: '编辑',
    mac: 'Cmd+L',
    win: 'Ctrl+L',
  },

  // ─── Agent ───
  {
    id: 'toggle-focus-mode',
    label: '专注模式',
    group: 'Agent',
    mac: 'Alt+F',
    win: 'Alt+F',
  },
]

/** 快速查找表 */
const shortcutMap = new Map(SHORTCUT_DEFINITIONS.map((d) => [d.id, d]))

/**
 * 获取指定快捷键的定义
 */
export function getShortcutDefinition(id: string): ShortcutDefinition | undefined {
  return shortcutMap.get(id)
}

/**
 * 获取指定快捷键在当前平台的快捷键字符串
 */
export function getShortcutForPlatform(id: string): string | undefined {
  const def = shortcutMap.get(id)
  if (!def) return undefined
  return isMac ? def.mac : def.win
}

/**
 * 获取按分组组织的所有快捷键
 */
export function getShortcutsByGroup(): Record<string, ShortcutDefinition[]> {
  const groups: Record<string, ShortcutDefinition[]> = {}
  for (const def of SHORTCUT_DEFINITIONS) {
    if (!groups[def.group]) groups[def.group] = []
    groups[def.group].push(def)
  }
  return groups
}

/**
 * 将快捷键字符串格式化为平台可读形式
 * 例如 Mac: "Cmd+Shift+K" → "⌘⇧K"
 */
export function formatShortcut(shortcut: string): string {
  if (isMac) {
    return shortcut
      .replace(/Cmd\+/g, '⌘')
      .replace(/Shift\+/g, '⇧')
      .replace(/Alt\+/g, '⌥')
      .replace(/Ctrl\+/g, '⌃')
  }
  return shortcut
}

/** A single visual token inside a kbd cluster. `mod` = modifier (rendered
 *  with the larger / dedicated style); `key` = the final key (letter,
 *  digit, punctuation, named key like Enter / Escape). */
export interface ShortcutToken {
  kind: 'mod' | 'key'
  /** What to render in the kbd cell — already platform-translated
   *  (Mac → glyphs ⌘ ⇧ ⌥ ⌃; Windows → "Ctrl" / "Shift" / "Alt" text). */
  display: string
}

/**
 * Parse a uClaw shortcut string ("Cmd+Shift+P" / "Alt+F" / "Cmd+1") into
 * a list of tokens for the kbd-cluster UI. Each modifier becomes its own
 * `mod` token, the final part becomes a `key` token. Empty / null input
 * returns an empty array (the UI shows "unbound" state).
 */
export function parseShortcutTokens(shortcut: string | undefined | null): ShortcutToken[] {
  if (!shortcut) return []
  const parts = shortcut.split('+').map((p) => p.trim()).filter(Boolean)
  if (parts.length === 0) return []
  const last = parts[parts.length - 1]!
  const mods = parts.slice(0, -1)
  const tokens: ShortcutToken[] = mods.map((m) => ({
    kind: 'mod',
    display: modSymbol(m),
  }))
  tokens.push({ kind: 'key', display: keySymbol(last) })
  return tokens
}

function modSymbol(mod: string): string {
  const m = mod.toLowerCase()
  if (isMac) {
    if (m === 'cmd' || m === 'meta' || m === '⌘') return '⌘'
    if (m === 'shift' || m === '⇧') return '⇧'
    if (m === 'alt' || m === 'option' || m === '⌥') return '⌥'
    if (m === 'ctrl' || m === 'control' || m === '⌃') return '⌃'
  }
  // Windows / Linux: keep the words
  if (m === 'cmd' || m === 'meta') return 'Meta'
  if (m === 'ctrl' || m === 'control') return 'Ctrl'
  if (m === 'shift') return 'Shift'
  if (m === 'alt' || m === 'option') return 'Alt'
  return mod
}

function keySymbol(key: string): string {
  // Named keys → friendly glyphs / labels
  const named: Record<string, string> = {
    arrowleft: '←',
    arrowright: '→',
    arrowup: '↑',
    arrowdown: '↓',
    escape: 'Esc',
    enter: '↵',
    backspace: '⌫',
    space: 'Space',
    tab: '⇥',
  }
  const lower = key.toLowerCase()
  if (named[lower]) return named[lower]
  return key.length === 1 ? key.toUpperCase() : key
}

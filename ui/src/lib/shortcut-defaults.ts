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
    id: 'close-tab',
    label: '关闭标签页',
    group: '导航',
    mac: 'Cmd+W',
    win: 'Ctrl+W',
  },
  {
    id: 'next-tab',
    label: '下一个标签页',
    group: '导航',
    mac: 'Cmd+]',
    win: 'Ctrl+Tab',
  },
  {
    id: 'prev-tab',
    label: '上一个标签页',
    group: '导航',
    mac: 'Cmd+[',
    win: 'Ctrl+Shift+Tab',
  },
  {
    id: 'toggle-sidebar',
    label: '切换侧边栏',
    group: '导航',
    mac: 'Cmd+B',
    win: 'Ctrl+B',
  },
  {
    id: 'open-settings',
    label: '打开设置',
    group: '导航',
    mac: 'Cmd+,',
    win: 'Ctrl+,',
  },
  {
    id: 'open-shortcuts',
    label: '快捷键设置',
    group: '导航',
    mac: 'Cmd+K Cmd+S',
    win: 'Ctrl+K Ctrl+S',
  },

  // ─── 工作区切换 ───
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
  {
    id: 'search-conversations',
    label: '搜索对话',
    group: '搜索',
    mac: 'Cmd+Shift+F',
    win: 'Ctrl+Shift+F',
  },

  // ─── 编辑 ───
  {
    id: 'focus-input',
    label: '聚焦输入框',
    group: '编辑',
    mac: 'Cmd+L',
    win: 'Ctrl+L',
  },
  {
    id: 'clear-input',
    label: '清空输入',
    group: '编辑',
    mac: 'Cmd+Shift+Backspace',
    win: 'Ctrl+Shift+Backspace',
  },

  // ─── Agent ───
  {
    id: 'stop-generation',
    label: '停止生成',
    group: 'Agent',
    mac: 'Escape',
    win: 'Escape',
  },
  {
    id: 'toggle-thinking',
    label: '切换思考模式',
    group: 'Agent',
    mac: 'Cmd+Shift+T',
    win: 'Ctrl+Shift+T',
  },
  {
    id: 'toggle-side-panel',
    label: '切换侧面板',
    group: 'Agent',
    mac: 'Cmd+Shift+B',
    win: 'Ctrl+Shift+B',
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

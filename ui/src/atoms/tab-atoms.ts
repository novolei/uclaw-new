/**
 * Tab Atoms — 标签页状态管理
 *
 * Tabs are tagged with a workspaceId. The flat `tabsAtom` is the pool;
 * `visibleTabsAtom` is the derived per-workspace filter that the
 * TabBar / MainArea render from. The active-tab pointer is held in
 * `workspaceActiveTabIdMapAtom` (one slot per workspace) and exposed
 * via the read-write `activeTabIdAtom` that reads/writes the slot for
 * the currently-active workspace.
 *
 * In-memory only — no persistence across app restarts (today's
 * behavior; not regressing).
 */

import { atom } from 'jotai'
import { activeWorkspaceIdAtom } from './workspace'
import {
  streamingConversationIdsAtom,
} from './chat-atoms'
import {
  agentRunningSessionIdsAtom,
  agentSessionIndicatorMapAtom,
  workingDoneSessionIdsAtom,
} from './agent-atoms'
import type { SessionIndicatorStatus } from './agent-atoms'

// ===== 类型定义 =====

export type TabType = 'chat' | 'agent' | 'browser'

export interface TabItem {
  id: string
  type: TabType
  sessionId: string
  title: string
  workspaceId: string
}

export interface PersistedTabState {
  tabs: TabItem[]
  activeTabId: string | null
}

// ===== 核心 Atoms =====

/** Pool of every open tab across all workspaces. */
export const tabsAtom = atom<TabItem[]>([])

/** Per-workspace active-tab pointer. Map<workspaceId, tabId | null>. */
export const workspaceActiveTabIdMapAtom =
  atom<Map<string, string | null>>(new Map())

/** Tabs visible in the currently-active workspace. Derived filter. */
export const visibleTabsAtom = atom<TabItem[]>((get) => {
  const wsId = get(activeWorkspaceIdAtom)
  if (!wsId) return []
  return get(tabsAtom).filter((t) => t.workspaceId === wsId)
})

/**
 * Read/write the active workspace's active-tab id. Reads return the
 * slot for the currently-active workspace; writes update that slot
 * (and only that slot) in workspaceActiveTabIdMapAtom.
 *
 * Writing `null` removes the slot entry entirely (clears the
 * workspace's active-tab pointer). Writing when no workspace is
 * active is a silent no-op. Returns null when no workspace is active
 * or the workspace has no active tab on record.
 */
export const activeTabIdAtom = atom<string | null, [string | null], void>(
  (get) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return null
    return get(workspaceActiveTabIdMapAtom).get(wsId) ?? null
  },
  (get, set, next) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return
    const m = new Map(get(workspaceActiveTabIdMapAtom))
    if (next === null) m.delete(wsId)
    else m.set(wsId, next)
    set(workspaceActiveTabIdMapAtom, m)
  },
)

export const tabMruAtom = atom<string[]>([])

export interface TabMinimapItem {
  id: string
  role: 'user' | 'assistant' | 'status'
  preview: string
  avatar?: string
  model?: string
}
export const tabMinimapCacheAtom = atom<Map<string, TabMinimapItem[]>>(new Map())

// ===== 派生 Atoms =====

export const activeTabAtom = atom<TabItem | null>((get) => {
  const activeId = get(activeTabIdAtom)
  if (!activeId) return null
  return get(tabsAtom).find((t) => t.id === activeId) ?? null
})

export const tabStreamingMapAtom = atom<Map<string, boolean>>((get) => {
  const tabs = get(tabsAtom)
  const chatStreaming = get(streamingConversationIdsAtom)
  const agentRunning = get(agentRunningSessionIdsAtom)
  const map = new Map<string, boolean>()
  for (const tab of tabs) {
    if (tab.type === 'chat') {
      map.set(tab.id, chatStreaming.has(tab.sessionId))
    } else if (tab.type === 'agent') {
      map.set(tab.id, agentRunning.has(tab.sessionId))
    } else if (tab.type === 'browser') {
      map.set(tab.id, false)
    }
  }
  return map
})

export const tabIndicatorMapAtom = atom<Map<string, SessionIndicatorStatus>>((get) => {
  const tabs = get(tabsAtom)
  const chatStreaming = get(streamingConversationIdsAtom)
  const agentIndicator = get(agentSessionIndicatorMapAtom)
  const workingDoneIds = get(workingDoneSessionIdsAtom)
  const map = new Map<string, SessionIndicatorStatus>()
  for (const tab of tabs) {
    if (tab.type === 'chat') {
      map.set(tab.id, chatStreaming.has(tab.sessionId) ? 'running' : 'idle')
    } else if (tab.type === 'agent') {
      const status = agentIndicator.get(tab.sessionId)
        ?? (workingDoneIds.has(tab.sessionId) ? 'completed' : 'idle')
      map.set(tab.id, status)
    } else if (tab.type === 'browser') {
      map.set(tab.id, 'idle')
    }
  }
  return map
})

// ===== 操作函数 =====

export function openTab(
  tabs: TabItem[],
  item: { type: TabType; sessionId: string; title: string; workspaceId: string },
): { tabs: TabItem[]; activeTabId: string } {
  const existingTab = tabs.find(
    (t) => t.sessionId === item.sessionId && t.type === item.type,
  )
  if (existingTab) {
    return { tabs, activeTabId: existingTab.id }
  }
  const newTab: TabItem = {
    id: item.sessionId,
    type: item.type,
    sessionId: item.sessionId,
    title: item.title,
    workspaceId: item.workspaceId,
  }
  return {
    tabs: [...tabs, newTab],
    activeTabId: newTab.id,
  }
}

export function closeTab(
  tabs: TabItem[],
  activeTabId: string | null,
  tabId: string,
): { tabs: TabItem[]; activeTabId: string | null } {
  const tabIndex = tabs.findIndex((t) => t.id === tabId)
  if (tabIndex === -1) return { tabs, activeTabId }
  const newTabs = tabs.filter((t) => t.id !== tabId)
  let newActiveTabId = activeTabId
  if (activeTabId === tabId) {
    if (newTabs.length > 0) {
      const nextIndex = Math.min(tabIndex, newTabs.length - 1)
      newActiveTabId = newTabs[nextIndex]!.id
    } else {
      newActiveTabId = null
    }
  }
  return { tabs: newTabs, activeTabId: newActiveTabId }
}

export function reorderTabs(
  tabs: TabItem[],
  fromIndex: number,
  toIndex: number,
): TabItem[] {
  if (fromIndex === toIndex) return tabs
  const newTabs = [...tabs]
  const [moved] = newTabs.splice(fromIndex, 1)
  newTabs.splice(toIndex, 0, moved!)
  return newTabs
}

export function updateTabTitle(
  tabs: TabItem[],
  sessionId: string,
  title: string,
): TabItem[] {
  return tabs.map((t) =>
    t.sessionId === sessionId ? { ...t, title } : t,
  )
}

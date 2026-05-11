/**
 * Tab Atoms — 标签页状态管理
 *
 * 支持浏览器风格的多标签页。
 * 通过桥接 atom 与现有 currentConversationIdAtom / currentAgentSessionIdAtom 同步。
 */

import { atom } from 'jotai'
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
}

export interface PersistedTabState {
  tabs: TabItem[]
  activeTabId: string | null
}

// ===== 核心 Atoms =====

export const tabsAtom = atom<TabItem[]>([])
export const activeTabIdAtom = atom<string | null>(null)
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
  item: { type: TabType; sessionId: string; title: string },
): { tabs: TabItem[]; activeTabId: string } {
  const existingTab = tabs.find((t) => t.sessionId === item.sessionId && t.type === item.type)
  if (existingTab) {
    return { tabs, activeTabId: existingTab.id }
  }
  const newTab: TabItem = {
    id: item.sessionId,
    type: item.type,
    sessionId: item.sessionId,
    title: item.title,
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
    t.sessionId === sessionId ? { ...t, title } : t
  )
}

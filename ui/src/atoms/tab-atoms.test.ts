import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  tabsAtom,
  activeTabIdAtom,
  visibleTabsAtom,
  workspaceActiveTabIdMapAtom,
  openTab,
  closeTab,
  reorderTabs,
  mergeReorderedVisible,
  type TabItem,
} from './tab-atoms'
import { activeWorkspaceIdAtom } from './workspace'

function tab(id: string, workspaceId: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId }
}

describe('tab-atoms — per-workspace memory', () => {
  it('visibleTabsAtom filters tabs by active workspace', () => {
    const store = createStore()
    store.set(tabsAtom, [tab('a1', 'ws-1'), tab('a2', 'ws-1'), tab('b1', 'ws-2')])
    store.set(activeWorkspaceIdAtom, 'ws-1')
    expect(store.get(visibleTabsAtom).map((t) => t.id)).toEqual(['a1', 'a2'])
    store.set(activeWorkspaceIdAtom, 'ws-2')
    expect(store.get(visibleTabsAtom).map((t) => t.id)).toEqual(['b1'])
  })

  it('activeTabIdAtom reads/writes the slot for the active workspace', () => {
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-1')
    store.set(activeTabIdAtom, 'a1')
    store.set(activeWorkspaceIdAtom, 'ws-2')
    expect(store.get(activeTabIdAtom)).toBeNull()
    store.set(activeTabIdAtom, 'b1')
    store.set(activeWorkspaceIdAtom, 'ws-1')
    expect(store.get(activeTabIdAtom)).toBe('a1')
  })

  it('openTab carries the supplied workspaceId onto the new tab', () => {
    const result = openTab([], {
      type: 'agent', sessionId: 's1', title: 't', workspaceId: 'ws-1',
    })
    expect(result.tabs[0]?.workspaceId).toBe('ws-1')
    expect(result.activeTabId).toBe('s1')
  })

  it('closeTab works by tab id (no workspaceId needed)', () => {
    const tabs = [tab('a1', 'ws-1'), tab('a2', 'ws-1')]
    const result = closeTab(tabs, 'a1', 'a1')
    expect(result.tabs.map((t) => t.id)).toEqual(['a2'])
    expect(result.activeTabId).toBe('a2')
  })

  it('writing null to activeTabIdAtom clears the slot for the active workspace', () => {
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-1')
    store.set(activeTabIdAtom, 'a1')
    expect(store.get(workspaceActiveTabIdMapAtom).has('ws-1')).toBe(true)
    store.set(activeTabIdAtom, null)
    expect(store.get(activeTabIdAtom)).toBeNull()
    expect(store.get(workspaceActiveTabIdMapAtom).has('ws-1')).toBe(false)
  })

  it('writing activeTabIdAtom is a no-op when no workspace is active', () => {
    const store = createStore()
    // No activeWorkspaceIdAtom set — should default to null.
    store.set(activeTabIdAtom, 'a1')
    expect(store.get(workspaceActiveTabIdMapAtom).size).toBe(0)
    expect(store.get(activeTabIdAtom)).toBeNull()
  })
})

describe('mergeReorderedVisible', () => {
  it('splices the reordered visible slice back without disturbing other workspaces', () => {
    const all: TabItem[] = [
      tab('a1', 'ws-1'),
      tab('b1', 'ws-2'),
      tab('a2', 'ws-1'),
      tab('b2', 'ws-2'),
    ]
    // Reorder ws-1 tabs: a2 before a1
    const reorderedVisible = [tab('a2', 'ws-1'), tab('a1', 'ws-1')]
    const merged = mergeReorderedVisible(all, reorderedVisible)

    // ws-1 tabs are reordered in place
    expect(merged[0]!.id).toBe('a2')
    expect(merged[2]!.id).toBe('a1')
    // ws-2 tabs are untouched
    expect(merged[1]!.id).toBe('b1')
    expect(merged[3]!.id).toBe('b2')
  })

  it('is identity when there is only one workspace', () => {
    const all: TabItem[] = [tab('a1', 'ws-1'), tab('a2', 'ws-1'), tab('a3', 'ws-1')]
    const reordered = reorderTabs(all, 0, 2)
    const merged = mergeReorderedVisible(all, reordered)
    expect(merged.map((t) => t.id)).toEqual(['a2', 'a3', 'a1'])
  })
})

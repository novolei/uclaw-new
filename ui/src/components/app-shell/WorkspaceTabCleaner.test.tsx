import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { WorkspaceTabCleaner } from './WorkspaceTabCleaner'
import {
  tabsAtom, workspaceActiveTabIdMapAtom, type TabItem,
} from '@/atoms/tab-atoms'
import { workspacesAtom, type WorkspaceInfo } from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({}))

function tab(id: string, workspaceId: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId }
}
function ws(id: string): WorkspaceInfo {
  return {
    id, name: id, icon: 'Folder', path: `/${id}`,
    attachedDirs: [], sortOrder: 0,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

describe('WorkspaceTabCleaner', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('drops tabs whose workspaceId no longer exists', () => {
    const store = createStore()
    store.set(tabsAtom, [tab('a1', 'ws-1'), tab('b1', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([
      ['ws-1', 'a1'], ['ws-2', 'b1'],
    ]))
    store.set(workspacesAtom, [ws('ws-1'), ws('ws-2')])

    const { rerender } = render(<Provider store={store}><WorkspaceTabCleaner /></Provider>)
    // No deletion yet → nothing pruned
    expect(store.get(tabsAtom)).toHaveLength(2)

    // Simulate deletion: workspacesAtom shrinks to drop ws-2.
    store.set(workspacesAtom, [ws('ws-1')])
    rerender(<Provider store={store}><WorkspaceTabCleaner /></Provider>)

    expect(store.get(tabsAtom).map((t) => t.id)).toEqual(['a1'])
    expect(store.get(workspaceActiveTabIdMapAtom).has('ws-2')).toBe(false)
    expect(store.get(workspaceActiveTabIdMapAtom).has('ws-1')).toBe(true)
  })

  it('is a no-op when no workspaces have been deleted', () => {
    const store = createStore()
    store.set(tabsAtom, [tab('a1', 'ws-1')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'a1']]))
    store.set(workspacesAtom, [ws('ws-1')])

    render(<Provider store={store}><WorkspaceTabCleaner /></Provider>)
    expect(store.get(tabsAtom)).toHaveLength(1)
    expect(store.get(workspaceActiveTabIdMapAtom).size).toBe(1)
  })
})

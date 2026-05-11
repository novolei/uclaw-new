import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore, useAtomValue } from 'jotai'
import { render } from '@testing-library/react'
import { GlobalShortcuts } from './GlobalShortcuts'
import { workspacesAtom } from '@/atoms/workspace'
import type { WorkspaceInfo } from '@/atoms/workspace'
import { getShortcutForPlatform } from '@/lib/shortcut-defaults'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  updateWorkspace: vi.fn(),
  reorderWorkspaces: vi.fn(),
}))

function makeWs(id: string, name: string, sortOrder: number): WorkspaceInfo {
  return {
    id, name, icon: '📁',
    path: `/tmp/${id}`,
    attachedDirs: [],
    sortOrder,
    createdAt: '2026-05-11T00:00:00Z',
    updatedAt: '2026-05-11T00:00:00Z',
  }
}

function fireDigit(digit: number) {
  // In jsdom (non-Mac), Ctrl+digit triggers the shortcut.
  // On Mac it's Cmd+digit, but useShortcut hook normalizes for platform.
  const evt = new KeyboardEvent('keydown', {
    key: String(digit), ctrlKey: true, bubbles: true,
  })
  console.log('[test] firing key event:', { key: evt.key, ctrlKey: evt.ctrlKey, metaKey: evt.metaKey })
  window.dispatchEvent(evt)
}


describe('GlobalShortcuts: workspace shortcuts', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('shortcut definitions exist for switch-workspace-1..9', () => {
    for (let i = 1; i <= 9; i++) {
      expect(getShortcutForPlatform(`switch-workspace-${i}`)).toBe(`Ctrl+${i}`)
    }
  })

  it('GlobalShortcuts component renders without error with workspaces', async () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    const { unmount } = render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(() => unmount()).not.toThrow()
  })

  it('GlobalShortcuts component renders without error with empty workspaces', async () => {
    const store = createStore()
    store.set(workspacesAtom, [])
    const { unmount } = render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(() => unmount()).not.toThrow()
  })
})

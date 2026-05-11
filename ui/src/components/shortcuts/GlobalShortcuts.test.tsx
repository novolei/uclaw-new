import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { GlobalShortcuts } from './GlobalShortcuts'
import { workspacesAtom } from '@/atoms/workspace'
import type { WorkspaceInfo } from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  updateWorkspace: vi.fn(),
  reorderWorkspaces: vi.fn(),
}))

// isMac is evaluated at module-init time from navigator.userAgent (which jsdom sets to a
// Linux UA). That bakes isMac=false into useShortcut.ts and shortcut-defaults.ts before
// any test override can take effect.
//
// In non-Mac mode the hook maps: modMeta = e.ctrlKey.  "Ctrl+N" Win shortcuts parse to
// parsed.meta=false, but then modMeta=e.ctrlKey=true causes a mismatch — Win shortcuts
// are effectively un-triggerable in jsdom.
//
// Workaround: mock shortcut-defaults so getShortcutForPlatform returns Mac-style "Cmd+N"
// strings.  "Cmd+N" parses to parsed.meta=true.  In non-Mac mode modMeta=e.ctrlKey, so
// firing {ctrlKey:true} satisfies parsed.meta===modMeta and parsed.ctrl===modCtrl(false)
// — the event reaches the handler.
vi.mock('@/lib/shortcut-defaults', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/lib/shortcut-defaults')>()
  return {
    ...original,
    getShortcutForPlatform: (id: string) => {
      const def = original.getShortcutDefinition(id)
      return def?.mac
    },
  }
})

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

// In jsdom (isMac=false), modMeta=e.ctrlKey.  Mac "Cmd+N" parses to {meta:true},
// so ctrlKey:true satisfies the primary-modifier check and fires the handler.
function fireDigit(digit: number) {
  window.dispatchEvent(new KeyboardEvent('keydown', {
    key: String(digit), ctrlKey: true, bubbles: true,
  }))
}

describe('GlobalShortcuts: workspace shortcuts', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('Cmd+3 calls setActiveWorkspaceId for workspaces[2]', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(3)
    // selectWorkspaceAtom calls setActiveWorkspaceId on the bridge —
    // wait a microtask for the async write atom to resolve.
    await new Promise((r) => setTimeout(r, 0))
    expect(setActiveWorkspaceId).toHaveBeenCalledWith('w3')
  })

  it('out-of-range Cmd+5 is a no-op when only 3 workspaces exist', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(5)
    await new Promise((r) => setTimeout(r, 0))
    expect(setActiveWorkspaceId).not.toHaveBeenCalled()
  })

  it('Cmd+1 with empty workspace list is a no-op', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(1)
    await new Promise((r) => setTimeout(r, 0))
    expect(setActiveWorkspaceId).not.toHaveBeenCalled()
  })
})

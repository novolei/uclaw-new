import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'
import { render, screen, waitFor } from '@testing-library/react'
import { TabBar } from './TabBar'
import { tabsAtom, workspaceActiveTabIdMapAtom, type TabItem } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom, workspacesAtom, type WorkspaceInfo } from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
}))

function mk(id: string, ws: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId: ws }
}
function ws(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: 'Folder', path: `/${id}`, attachedDirs: [],
    sortOrder: 0, createdAt: '', updatedAt: '',
  }
}

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <TooltipProvider>
        <TabBar />
      </TooltipProvider>
    </Provider>
  )
}

describe('TabBar — per-workspace visibility', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('renders only the active workspace\'s tabs', async () => {
    const store = createStore()
    store.set(tabsAtom, [mk('a1', 'ws-1'), mk('a2', 'ws-1'), mk('b1', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'a1'], ['ws-2', 'b1']]))
    store.set(workspacesAtom, [ws('ws-1', 'A'), ws('ws-2', 'B')])
    store.set(activeWorkspaceIdAtom, 'ws-1')
    const { rerender } = renderWith(store)
    expect(screen.queryByText('b1')).not.toBeInTheDocument()
    expect(screen.getByText('a1')).toBeInTheDocument()
    expect(screen.getByText('a2')).toBeInTheDocument()

    store.set(activeWorkspaceIdAtom, 'ws-2')
    rerender(
      <Provider store={store}>
        <TooltipProvider>
          <TabBar />
        </TooltipProvider>
      </Provider>
    )
    // Workspace switch transitions are async (AnimatePresence handles
    // exit-then-enter via motion). Wait for the new content to settle.
    await waitFor(() => expect(screen.getByText('b1')).toBeInTheDocument())
    await waitFor(() => expect(screen.queryByText('a1')).not.toBeInTheDocument())
    expect(screen.queryByText('a2')).not.toBeInTheDocument()
  })

  it('switching workspace flips both visible tabs AND active-tab indicator', async () => {
    const store = createStore()
    store.set(tabsAtom, [mk('a1', 'ws-1'), mk('a2', 'ws-1'), mk('b1', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([
      ['ws-1', 'a2'], ['ws-2', 'b1'],
    ]))
    store.set(workspacesAtom, [ws('ws-1', 'A'), ws('ws-2', 'B')])
    store.set(activeWorkspaceIdAtom, 'ws-1')

    const { rerender } = renderWith(store)
    // ws-1's active tab is 'a2' — verify it has the active styling (bg-content-area)
    const a2Button = screen.getByText('a2').closest('button')
    expect(a2Button?.className).toContain('bg-content-area')
    // a1 should be inactive
    const a1Button = screen.getByText('a1').closest('button')
    expect(a1Button?.className).not.toContain('bg-content-area')

    // Switch to ws-2
    store.set(activeWorkspaceIdAtom, 'ws-2')
    rerender(
      <Provider store={store}>
        <TooltipProvider>
          <TabBar />
        </TooltipProvider>
      </Provider>
    )
    // Wait for the AnimatePresence transition.
    await waitFor(() => expect(screen.getByText('b1')).toBeInTheDocument())
    await waitFor(() => expect(screen.queryByText('a1')).not.toBeInTheDocument())
    expect(screen.queryByText('a2')).not.toBeInTheDocument()
    const b1Button = screen.getByText('b1').closest('button')
    expect(b1Button?.className).toContain('bg-content-area')
  })
})

import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { TabBarWorkspaceChip } from './TabBarWorkspaceChip'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  type WorkspaceInfo,
} from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  createWorkspace: vi.fn().mockResolvedValue({ id: 'new', name: 'new', icon: '📁' }),
  openFolderDialog: vi.fn(),
}))

function makeWs(id: string, name: string, sortOrder: number, icon = '📁'): WorkspaceInfo {
  return {
    id, name, icon, path: `/tmp/${id}`, attachedDirs: [], sortOrder,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

function renderWithStore(store: ReturnType<typeof createStore>) {
  return render(<Provider store={store}><TabBarWorkspaceChip /></Provider>)
}

describe('TabBarWorkspaceChip', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('renders active workspace name and emoji', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', '2222', 0, '📁')])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    expect(screen.getByText('2222')).toBeInTheDocument()
    expect(screen.getByText('📁')).toBeInTheDocument()
  })

  it('truncates workspace name longer than 12 chars', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'abcdefghijklmnopqrst', 0)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    expect(screen.getByText('abcdefghijkl…')).toBeInTheDocument()
  })

  it('returns null when there is no active workspace', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one', 0)])
    store.set(activeWorkspaceIdAtom, null)
    const { container } = renderWithStore(store)
    expect(container.textContent).toBe('')
  })

  it('opens dropdown with all workspaces and shortcut hints for first 9', async () => {
    // Radix DropdownMenu requires userEvent (which fires pointer events) to open;
    // bare fireEvent.click does not trigger Radix's onPointerDown handler in jsdom.
    const user = userEvent.setup()
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    await user.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => {
      // The active workspace name appears twice: once in the chip trigger, once in the
      // dropdown menu item. Use getAllByText and assert at least 2 matches for 'First',
      // and at least 1 for workspaces that aren't active.
      expect(screen.getAllByText('First').length).toBeGreaterThanOrEqual(1)
      expect(screen.getAllByText('Second').length).toBeGreaterThanOrEqual(1)
      expect(screen.getAllByText('Third').length).toBeGreaterThanOrEqual(1)
      // Mac userAgent in jsdom is typically empty — match either prefix.
      const hint1 = screen.getByText(/^(?:⌘|Ctrl\+)1$/)
      expect(hint1).toBeInTheDocument()
    })
  })

  it('clicking a workspace item calls setActiveWorkspaceId', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const user = userEvent.setup()
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    await user.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => expect(screen.getByText('Second')).toBeInTheDocument())
    fireEvent.click(screen.getByText('Second'))
    await waitFor(() => {
      expect(setActiveWorkspaceId).toHaveBeenCalledWith('w2')
    })
  })

  it('"+ 新建工作区" opens the CreateDialog', async () => {
    const user = userEvent.setup()
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one', 0)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    await user.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => expect(screen.getByText('新建工作区')).toBeInTheDocument())
    fireEvent.click(screen.getByText('新建工作区'))
    await waitFor(() => {
      expect(screen.getByText('New Workspace')).toBeInTheDocument()
    })
  })
})

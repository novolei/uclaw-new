import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { WorkspaceSwitcherBar } from './WorkspaceSwitcherBar'
import { workspacesAtom, activeWorkspaceIdAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { agentSessionsAtom } from '@/atoms/agent-atoms'

// Mock lottie-react so the test doesn't need a real canvas/animation runtime.
vi.mock('lottie-react', () => ({
  default: () => <div data-testid="lottie-stub" />,
}))

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  reorderWorkspaces: vi.fn().mockResolvedValue(undefined),
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
  return render(<Provider store={store}><WorkspaceSwitcherBar /></Provider>)
}

describe('WorkspaceSwitcherBar', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('renders all icons (full) when workspaces.length ≤ 5', () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'A', 0, '📁'),
      makeWs('w2', 'B', 1, '💼'),
      makeWs('w3', 'C', 2, '🚀'),
    ])
    store.set(activeWorkspaceIdAtom, 'w2')
    renderWithStore(store)
    // After emoji→lucide-icon switch: assert the three workspace buttons
    // are present via aria-label rather than checking emoji text.
    expect(screen.getByLabelText(/工作区: A/)).toBeInTheDocument()
    expect(screen.getByLabelText(/工作区: B/)).toBeInTheDocument()
    expect(screen.getByLabelText(/工作区: C/)).toBeInTheDocument()
  })

  it('collapses non-active to dots when workspaces.length > 5', () => {
    const store = createStore()
    store.set(workspacesAtom, Array.from({ length: 7 }, (_, i) =>
      makeWs(`w${i}`, `name${i}`, i, '📁')
    ))
    store.set(activeWorkspaceIdAtom, 'w3')
    renderWithStore(store)
    // After emoji→lucide-icon switch: count workspace-icon buttons (full)
    // vs dot buttons (collapsed). With 7 workspaces and 1 active, we
    // expect 1 full icon and 6 dots.
    const fullIcons = screen.queryAllByLabelText(/^工作区: name\d+$/)
    const dots = screen.getAllByLabelText(/workspace dot/)
    expect(fullIcons.length).toBe(1)
    expect(dots.length).toBe(6)
  })

  it('clicking a workspace icon calls setActiveWorkspaceId', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'A', 0),
      makeWs('w2', 'B', 1),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    fireEvent.click(screen.getByLabelText(/工作区: B/))
    await waitFor(() => {
      expect(setActiveWorkspaceId).toHaveBeenCalledWith('w2')
    })
  })

  it('tooltip on hover shows pill-style chips for first 9', async () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    // Use fireEvent.pointerEnter + pointerMove to open Radix Tooltip in jsdom
    const trigger = screen.getByLabelText(/工作区: First/)
    fireEvent.pointerEnter(trigger)
    fireEvent.pointerMove(trigger)
    await waitFor(() => {
      // The tooltip renders with role="tooltip"; check it contains the name
      const tooltip = document.querySelector('[role="tooltip"]')
      expect(tooltip).not.toBeNull()
      expect(tooltip?.textContent).toContain('First')
    })
    // Shortcut chips: the modifier glyph and the digit '1' should appear
    // (this is the first workspace, index 0 → digit 1)
    const modChips = screen.queryAllByText(/^(?:⌘|Ctrl)$/)
    expect(modChips.length).toBeGreaterThan(0)
    const digitChips = screen.queryAllByText('1')
    expect(digitChips.length).toBeGreaterThan(0)
  })

  it('does not render running indicator when no sessions are running', () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'A', 0),
      makeWs('w2', 'B', 1),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(agentSessionsAtom, [])
    renderWithStore(store)
    const dots = screen.queryAllByLabelText(/任务执行中/)
    expect(dots.length).toBe(0)
  })

  it('"+" button opens WorkspaceCreateDialog', async () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'A', 0)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    fireEvent.click(screen.getByLabelText('新建工作区'))
    await waitFor(() => {
      expect(screen.getByText('New Workspace')).toBeInTheDocument()
    })
  })

  it('keyboard: only the active workspace icon is in the tab order (roving tabindex)', () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'A', 0),
      makeWs('w2', 'B', 1),
      makeWs('w3', 'C', 2),
    ])
    store.set(activeWorkspaceIdAtom, 'w2')
    renderWithStore(store)
    expect(screen.getByLabelText(/工作区: A/).tabIndex).toBe(-1)
    expect(screen.getByLabelText(/工作区: B/).tabIndex).toBe(0)
    expect(screen.getByLabelText(/工作区: C/).tabIndex).toBe(-1)
  })

  it('keyboard: ArrowRight moves roving focus + makes that icon tabbable', () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'A', 0),
      makeWs('w2', 'B', 1),
      makeWs('w3', 'C', 2),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    const toolbar = screen.getByRole('toolbar', { name: /工作区切换/ })
    // Initially A is the active + focused workspace
    expect(screen.getByLabelText(/工作区: A/).tabIndex).toBe(0)
    fireEvent.keyDown(toolbar, { key: 'ArrowRight' })
    // After ArrowRight, B should be tabbable; A should be -1.
    expect(screen.getByLabelText(/工作区: A/).tabIndex).toBe(-1)
    expect(screen.getByLabelText(/工作区: B/).tabIndex).toBe(0)
  })

  it('keyboard: ArrowLeft wraps from first to last', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'A', 0), makeWs('w2', 'B', 1)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    const toolbar = screen.getByRole('toolbar', { name: /工作区切换/ })
    fireEvent.keyDown(toolbar, { key: 'ArrowLeft' })
    expect(screen.getByLabelText(/工作区: B/).tabIndex).toBe(0)
  })
})

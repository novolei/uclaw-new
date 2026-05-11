import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen, fireEvent } from '@testing-library/react'
import { WorkspaceRail } from './WorkspaceRail'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  type WorkspaceInfo,
} from '@/atoms/workspace'
import { agentSessionsAtom } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn(),
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
}))

function makeWs(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: '📁', path: '/tmp', attachedDirs: [], sortOrder: 0,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

describe('WorkspaceRail (active workspace only)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('renders only the active workspace sessions', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'A'), makeWs('w2', 'B')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, {
      w1: [
        { id: 's1', title: 'In w1', titleEmoji: '💬', titlePending: false, spaceId: 'w1', updatedAt: '2026-05-11T00:00:00Z', pinnedAt: null },
      ],
      w2: [
        { id: 's2', title: 'In w2', titleEmoji: '💬', titlePending: false, spaceId: 'w2', updatedAt: '2026-05-11T00:00:00Z', pinnedAt: null },
      ],
    })
    store.set(agentSessionsAtom, [])
    render(
      <Provider store={store}>
        <WorkspaceRail activeSessionId={null} onSelectSession={() => {}} />
      </Provider>
    )
    expect(screen.getByText('In w1')).toBeInTheDocument()
    expect(screen.queryByText('In w2')).not.toBeInTheDocument()
  })

  it('shows empty-state hint when active workspace has no sessions', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'A')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, { w1: [] })
    store.set(agentSessionsAtom, [])
    render(
      <Provider store={store}>
        <WorkspaceRail activeSessionId={null} onSelectSession={() => {}} />
      </Provider>
    )
    expect(screen.getByText(/尚无会话/)).toBeInTheDocument()
  })

  it('clicking a session calls onSelectSession with its id', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'A')])
    store.set(activeWorkspaceIdAtom, 'w1')
    store.set(workspaceSessionsAtom, {
      w1: [
        { id: 's-click', title: 'Pick me', titleEmoji: '💬', titlePending: false, spaceId: 'w1', updatedAt: '2026-05-11T00:00:00Z', pinnedAt: null },
      ],
    })
    store.set(agentSessionsAtom, [])
    const onSelect = vi.fn()
    render(
      <Provider store={store}>
        <WorkspaceRail activeSessionId={null} onSelectSession={onSelect} />
      </Provider>
    )
    fireEvent.click(screen.getByText('Pick me'))
    expect(onSelect).toHaveBeenCalledWith('s-click')
  })
})

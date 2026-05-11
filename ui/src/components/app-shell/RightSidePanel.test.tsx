import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { RightSidePanel } from './RightSidePanel'
import { activeWorkspaceIdAtom, workspacesAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { currentAgentSessionIdAtom, agentSessionPathMapAtom, workspaceActiveRightPanelTabMapAtom } from '@/atoms/agent-atoms'
import { appModeAtom } from '@/atoms/app-mode'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  setActiveWorkspaceId: vi.fn(),
  listDirectoryEntries: vi.fn().mockResolvedValue([]),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

function makeWs(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: '📁', path: `/tmp/${id}`, attachedDirs: [], sortOrder: 0,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

function seed(store: ReturnType<typeof createStore>, opts: { activeWs: string; sessionId: string | null }) {
  store.set(appModeAtom, 'agent')
  store.set(workspacesAtom, [makeWs('w1', 'A'), makeWs('w2', 'B')])
  store.set(activeWorkspaceIdAtom, opts.activeWs)
  store.set(currentAgentSessionIdAtom, opts.sessionId)
  store.set(agentSessionPathMapAtom, new Map([[opts.sessionId ?? '', '/tmp/path']]))
}

describe('RightSidePanel per-workspace tab memory', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('defaults to Files tab when no entry in map', () => {
    const store = createStore()
    seed(store, { activeWs: 'w1', sessionId: 's1' })
    render(<Provider store={store}><RightSidePanel /></Provider>)
    const filesBtn = screen.getByTitle('Files')
    expect(filesBtn.className).toMatch(/bg-primary/)
  })

  it('clicking a tab writes per-workspace entry', () => {
    const store = createStore()
    seed(store, { activeWs: 'w1', sessionId: 's1' })
    render(<Provider store={store}><RightSidePanel /></Provider>)
    fireEvent.click(screen.getByTitle('Plan'))
    const map = store.get(workspaceActiveRightPanelTabMapAtom)
    expect(map.get('w1')).toBe('plan')
  })

  it('switching workspace restores that workspace previous tab', async () => {
    const store = createStore()
    seed(store, { activeWs: 'w1', sessionId: 's1' })
    const { rerender } = render(<Provider store={store}><RightSidePanel /></Provider>)
    fireEvent.click(screen.getByTitle('Plan'))
    expect(store.get(workspaceActiveRightPanelTabMapAtom).get('w1')).toBe('plan')

    store.set(activeWorkspaceIdAtom, 'w2')
    rerender(<Provider store={store}><RightSidePanel /></Provider>)
    const filesBtnAfterSwitch = screen.getByTitle('Files')
    expect(filesBtnAfterSwitch.className).toMatch(/bg-primary/)

    store.set(activeWorkspaceIdAtom, 'w1')
    rerender(<Provider store={store}><RightSidePanel /></Provider>)
    await waitFor(() => {
      const planBtn = screen.getByTitle('Plan')
      expect(planBtn.className).toMatch(/bg-primary/)
    })
  })
})

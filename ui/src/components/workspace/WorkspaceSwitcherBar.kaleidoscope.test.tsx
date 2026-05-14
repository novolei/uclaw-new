import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { WorkspaceSwitcherBar } from './WorkspaceSwitcherBar'
import { workspacesAtom, activeWorkspaceIdAtom, type WorkspaceInfo } from '@/atoms/workspace'
import { topLevelViewAtom } from '@/atoms/top-level-view'

// Mock lottie-react so the test doesn't need a real canvas/animation runtime.
vi.mock('lottie-react', () => ({
  default: () => <div data-testid="lottie-stub" />,
}))

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn(),
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
}))

function makeWs(id: string, name: string): WorkspaceInfo {
  return {
    id, name, icon: 'Folder', path: `/tmp/${id}`, attachedDirs: [], sortOrder: 0,
    createdAt: '2026-05-14T00:00:00Z', updatedAt: '2026-05-14T00:00:00Z',
  }
}

describe('WorkspaceSwitcherBar — Kaleidoscope entry', () => {
  it('renders the Kaleidoscope entry icon', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one')])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithProviders(<WorkspaceSwitcherBar />, { store })
    expect(screen.getByRole('button', { name: '打开万花筒' })).toBeInTheDocument()
  })

  it('clicking the entry icon sets topLevelViewAtom to "kaleidoscope"', async () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one')])
    store.set(activeWorkspaceIdAtom, 'w1')
    const { user } = renderWithProviders(<WorkspaceSwitcherBar />, { store })
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(store.get(topLevelViewAtom)).toBe('kaleidoscope')
  })
})

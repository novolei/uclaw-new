import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { MainArea } from './MainArea'
import { topLevelViewAtom } from '@/atoms/top-level-view'

// Stub the two surfaces — MainArea's only job is to pick between them.
vi.mock('@/views/Workspace/WorkspaceShell', () => ({
  WorkspaceShell: () => <div data-testid="workspace-shell" />,
}))
vi.mock('@/views/Kaleidoscope/KaleidoscopeShell', () => ({
  KaleidoscopeShell: () => <div data-testid="kaleidoscope-shell" />,
}))
vi.mock('@/components/settings/SettingsDialog', () => ({
  SettingsDialog: () => null,
}))

describe('MainArea — top-level surface switch', () => {
  it('renders WorkspaceShell when topLevelView is "workspace" (default)', () => {
    renderWithProviders(<MainArea />)
    expect(screen.getByTestId('workspace-shell')).toBeInTheDocument()
    expect(screen.queryByTestId('kaleidoscope-shell')).not.toBeInTheDocument()
  })

  it('renders KaleidoscopeShell when topLevelView is "kaleidoscope"', () => {
    const store = createStore()
    store.set(topLevelViewAtom, 'kaleidoscope')
    renderWithProviders(<MainArea />, { store })
    expect(screen.getByTestId('kaleidoscope-shell')).toBeInTheDocument()
    expect(screen.queryByTestId('workspace-shell')).not.toBeInTheDocument()
  })
})

import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeShell } from './KaleidoscopeShell'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
}))

// AutomationHub pulls a heavy subtree — stub it; this test only cares that
// KaleidoscopeShell routes the right module into the main area.
vi.mock('@/components/automation/AutomationHub', () => ({
  AutomationHub: () => <div data-testid="automation-hub" />,
}))

describe('KaleidoscopeShell', () => {
  it('renders the rail and the humans module by default', () => {
    renderWithProviders(<KaleidoscopeShell />)
    expect(screen.getByRole('button', { name: /数字人/ })).toBeInTheDocument()
    expect(screen.getByTestId('automation-hub')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a non-humans module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
})

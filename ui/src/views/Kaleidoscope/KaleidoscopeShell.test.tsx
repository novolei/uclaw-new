import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeShell } from './KaleidoscopeShell'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
  listAutomationsHumane: vi.fn().mockResolvedValue([]),
}))

// automation 组件子树重 —— stub 掉，本测试只关心 KaleidoscopeShell 的模块路由。
vi.mock('@/components/automation/AutomationHub', () => ({
  AutomationHub: () => <div data-testid="automation-hub" />,
}))
vi.mock('@/components/automation/StoreView', () => ({
  StoreView: () => <div data-testid="store-view" />,
}))
vi.mock('@/components/automation/StoreDetail', () => ({
  StoreDetail: () => <div data-testid="store-detail" />,
}))
vi.mock('@/components/automation/AppsTab', () => ({
  AppsTab: () => <div data-testid="apps-tab" />,
}))

describe('KaleidoscopeShell', () => {
  it('renders the rail and the humans module (AutomationHub) by default', () => {
    renderWithProviders(<KaleidoscopeShell />)
    expect(screen.getByRole('button', { name: /数字人/ })).toBeInTheDocument()
    expect(screen.getByTestId('automation-hub')).toBeInTheDocument()
  })

  it('renders StoreView for the store module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'store')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('store-view')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })

  it('renders AppsTab for the apps module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'apps')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByTestId('apps-tab')).toBeInTheDocument()
  })

  it('renders the ComingSoon placeholder for a not-yet-built module', () => {
    const store = createStore()
    store.set(kaleidoscopeModuleAtom, 'skills')
    renderWithProviders(<KaleidoscopeShell />, { store })
    expect(screen.getByText('即将到来 · Phase 2')).toBeInTheDocument()
    expect(screen.queryByTestId('automation-hub')).not.toBeInTheDocument()
  })
})

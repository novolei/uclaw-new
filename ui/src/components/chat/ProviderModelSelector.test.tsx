import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { createStore } from 'jotai'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { ProviderModelSelector } from './ProviderModelSelector'

vi.mock('@/lib/tauri-bridge', () => ({
  getAllConfiguredModels: vi.fn(async () => []),
  getActiveModel: vi.fn(async () => null),
  setActiveModel: vi.fn(),
  setRoleModel: vi.fn(),
}))

// active-model atom reads from localStorage; silence any side-effects
vi.mock('@/atoms/active-model', () => ({
  activeProviderModelAtom: { init: null, read: () => null, write: () => {} },
}))

describe('ProviderModelSelector empty state', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders the 配置服务商 button when no models are configured', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<ProviderModelSelector />, { store })

    // Open the popover by clicking the trigger button
    await user.click(screen.getByRole('button', { name: /选择模型/i }))

    // The empty-state "配置服务商" button should appear
    const cfgBtn = await screen.findByRole('button', { name: /配置服务商/ })
    expect(cfgBtn).toBeInTheDocument()
  })

  it('configure-providers button opens settings to connectivity tab', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<ProviderModelSelector />, { store })

    // Open the popover
    await user.click(screen.getByRole('button', { name: /选择模型/i }))

    // Click the empty-state action button
    const cfgBtn = await screen.findByRole('button', { name: /配置服务商/ })
    await user.click(cfgBtn)

    await waitFor(() => {
      expect(store.get(settingsOpenAtom)).toBe(true)
      expect(store.get(settingsTabAtom)).toBe('connectivity')
    })
  })

  it('clicking 配置服务商 closes the popover', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<ProviderModelSelector />, { store })

    // Open the popover
    await user.click(screen.getByRole('button', { name: /选择模型/i }))
    const cfgBtn = await screen.findByRole('button', { name: /配置服务商/ })

    await user.click(cfgBtn)

    // Popover should be gone (the 配置服务商 button is no longer in the DOM)
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: /配置服务商/ })).not.toBeInTheDocument()
    })
  })
})

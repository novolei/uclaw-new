import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { KaleidoscopeRail } from './KaleidoscopeRail'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'

vi.mock('@/lib/tauri-bridge', () => ({
  getUserProfile: vi.fn().mockResolvedValue({ userName: 'User', avatar: null }),
}))

describe('KaleidoscopeRail', () => {
  it('renders all 7 module nav buttons', () => {
    renderWithProviders(<KaleidoscopeRail />)
    for (const label of ['数字人', '应用商店', '我的应用', '产出', '技能', '集成', '记忆']) {
      expect(screen.getByRole('button', { name: new RegExp(label) })).toBeInTheDocument()
    }
  })

  it('clicking a module button updates kaleidoscopeModuleAtom', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<KaleidoscopeRail />, { store })
    await user.click(screen.getByRole('button', { name: /技能/ }))
    expect(store.get(kaleidoscopeModuleAtom)).toBe('skills')
  })

  it('clicking the return button sets topLevelViewAtom back to "workspace"', async () => {
    const store = createStore()
    store.set(topLevelViewAtom, 'kaleidoscope')
    const { user } = renderWithProviders(<KaleidoscopeRail />, { store })
    await user.click(screen.getByRole('button', { name: '返回主窗口' }))
    expect(store.get(topLevelViewAtom)).toBe('workspace')
  })
})

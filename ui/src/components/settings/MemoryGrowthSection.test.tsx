import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'

const bridge = vi.hoisted(() => ({ getGrowthSettings: vi.fn(), setGrowthSettings: vi.fn() }))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { MemoryGrowthSection } from './MemoryGrowthSection'

beforeEach(() => {
  bridge.getGrowthSettings.mockReset().mockResolvedValue({ enabled: true, everyNTurns: 20, dailyBudget: 20000 })
  bridge.setGrowthSettings.mockReset().mockImplementation(async (s) => ({ ...s, everyNTurns: Math.max(5, s.everyNTurns) }))
})

describe('MemoryGrowthSection', () => {
  it('growth_settings_loads_and_saves_clamped_values', async () => {
    render(<MemoryGrowthSection />)
    const n = (await screen.findByLabelText('每 N 轮反思一次')) as HTMLInputElement
    expect(n.value).toBe('20')
    fireEvent.change(n, { target: { value: '2' } })
    fireEvent.click(screen.getByRole('button', { name: '保存' }))
    await waitFor(() =>
      expect(bridge.setGrowthSettings).toHaveBeenCalledWith({ enabled: true, everyNTurns: 2, dailyBudget: 20000 }),
    )
    await waitFor(() => expect(n.value).toBe('5'), { timeout: 2000 })
  })
})

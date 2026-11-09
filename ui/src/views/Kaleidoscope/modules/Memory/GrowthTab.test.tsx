import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'

const bridge = vi.hoisted(() => ({
  listGrowthReflections: vi.fn(),
  setGrowthReflectionArchived: vi.fn(),
  getGrowthUserModel: vi.fn(),
  listGrowthUserModelHistory: vi.fn(),
  refreshGrowthNow: vi.fn(),
}))
vi.mock('@/lib/tauri-bridge', () => bridge)

import { GrowthTab } from './GrowthTab'

beforeEach(() => {
  bridge.listGrowthReflections.mockReset().mockResolvedValue([
    { id: 'r1', insight: '偏好小而独立的 PR', confidence: 0.8, createdAt: 1, archivedAt: null },
  ])
  bridge.setGrowthReflectionArchived.mockReset().mockResolvedValue(true)
  bridge.getGrowthUserModel.mockReset().mockResolvedValue({ summary: '中文沟通的 Rust 工程师', updatedAt: 1 })
  bridge.listGrowthUserModelHistory.mockReset().mockResolvedValue([{ summary: '旧模型', replacedAt: 0 }])
  bridge.refreshGrowthNow.mockReset()
})

describe('GrowthTab', () => {
  it('growth_tab_shows_model_and_reflections', async () => {
    render(<GrowthTab />)
    expect(await screen.findByText('中文沟通的 Rust 工程师')).toBeInTheDocument()
    expect(screen.getByText('偏好小而独立的 PR')).toBeInTheDocument()
    expect(screen.getByText('0.80')).toBeInTheDocument()
  })

  it('archive_calls_bridge_and_refetches', async () => {
    render(<GrowthTab />)
    fireEvent.click(await screen.findByRole('button', { name: '归档' }))
    await waitFor(() => expect(bridge.setGrowthReflectionArchived).toHaveBeenCalledWith('r1', true))
    await waitFor(() => expect(bridge.listGrowthReflections).toHaveBeenCalledTimes(2))
  })

  it('refresh_shows_skip_reason', async () => {
    bridge.refreshGrowthNow.mockResolvedValue({ status: 'skipped', reason: 'no_llm' })
    render(<GrowthTab />)
    fireEvent.click(await screen.findByRole('button', { name: '立即刷新' }))
    expect(await screen.findByText(/未开启学习模型/)).toBeInTheDocument()
  })

  it('empty_state_explains_how_growth_starts', async () => {
    bridge.listGrowthReflections.mockResolvedValue([])
    bridge.getGrowthUserModel.mockResolvedValue(null)
    bridge.listGrowthUserModelHistory.mockResolvedValue([])
    render(<GrowthTab />)
    expect(await screen.findByText(/还没有用户模型/)).toBeInTheDocument()
    expect(screen.getByText(/还没有反思/)).toBeInTheDocument()
  })
})

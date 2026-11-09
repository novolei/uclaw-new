import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, waitFor, act } from '@testing-library/react'
import type { TurnCost } from '@/lib/types'

let turnCostCb: ((p: TurnCost) => void) | null = null

vi.mock('@/lib/tauri-bridge', () => ({
  getSessionCacheStats: vi.fn(async () => ({
    calls: 3, promptTokensTotal: 1000, cachedInputTokens: 820, hitRatio: 0.82,
  })),
  onTurnCost: vi.fn(async (cb: (p: TurnCost) => void) => {
    turnCostCb = cb
    return () => { turnCostCb = null }
  }),
}))

import { useSessionCacheStats, isCacheUnreported } from './useSessionCacheStats'

describe('useSessionCacheStats', () => {
  beforeEach(async () => {
    turnCostCb = null
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.getSessionCacheStats).mockClear()
  })

  it('loads the session stats on mount', async () => {
    const { result } = renderHook(() => useSessionCacheStats('s1'))
    await waitFor(() => expect(result.current?.hitRatio).toBe(0.82))
    const bridge = await import('@/lib/tauri-bridge')
    expect(bridge.getSessionCacheStats).toHaveBeenCalledWith('s1')
  })

  it('refetches after a turn_cost for this session only', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    renderHook(() => useSessionCacheStats('s1', 0))
    await waitFor(() => expect(turnCostCb).not.toBeNull())
    await waitFor(() => expect(bridge.getSessionCacheStats).toHaveBeenCalledTimes(1))
    act(() => {
      turnCostCb!({ conversationId: 'other', inputTokens: 1, outputTokens: 1, costUsd: '$0' })
    })
    await new Promise((r) => setTimeout(r, 10))
    expect(bridge.getSessionCacheStats).toHaveBeenCalledTimes(1)
    act(() => {
      turnCostCb!({ conversationId: 's1', inputTokens: 1, outputTokens: 1, costUsd: '$0' })
    })
    await waitFor(() => expect(bridge.getSessionCacheStats).toHaveBeenCalledTimes(2))
  })

  it('keeps undefined (no crash) when the command fails', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.getSessionCacheStats).mockRejectedValueOnce('boom')
    const { result } = renderHook(() => useSessionCacheStats('s2'))
    await new Promise((r) => setTimeout(r, 10))
    expect(result.current).toBeUndefined()
  })
})

describe('isCacheUnreported', () => {
  it('is true only when calls exist but no cache reads were ever reported', () => {
    expect(isCacheUnreported({ calls: 4, promptTokensTotal: 100, cachedInputTokens: 0, hitRatio: 0 })).toBe(true)
    expect(isCacheUnreported({ calls: 0, promptTokensTotal: 0, cachedInputTokens: 0, hitRatio: 0 })).toBe(false)
    expect(isCacheUnreported({ calls: 4, promptTokensTotal: 100, cachedInputTokens: 10, hitRatio: 0.1 })).toBe(false)
    expect(isCacheUnreported(undefined)).toBe(false)
  })
})

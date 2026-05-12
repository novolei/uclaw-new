import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  monthStartMsAtom,
  monthTotalUsdAtom,
  workspaceRollupAtom,
  firedBudgetThresholdsAtom,
  refreshCostsAtom,
} from './cost'

vi.mock('@/lib/tauri-bridge', () => ({
  getMonthCostTotal: vi.fn().mockResolvedValue(42.5),
  listWorkspaceCostRollup: vi.fn().mockResolvedValue([
    { workspaceId: 'ws-a', workspaceName: 'A', workspaceIcon: 'Folder', totalCostUsd: 30, totalTokens: 1000 },
    { workspaceId: 'ws-b', workspaceName: 'B', workspaceIcon: 'Folder', totalCostUsd: 12.5, totalTokens: 500 },
  ]),
}))

describe('cost atoms', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('monthStartMsAtom returns first-of-month midnight in local time', () => {
    const store = createStore()
    const ms = store.get(monthStartMsAtom)
    const d = new Date(ms)
    expect(d.getDate()).toBe(1)
    expect(d.getHours()).toBe(0)
    expect(d.getMinutes()).toBe(0)
    expect(d.getSeconds()).toBe(0)
    const now = new Date()
    expect(d.getMonth()).toBe(now.getMonth())
    expect(d.getFullYear()).toBe(now.getFullYear())
  })

  it('refreshCostsAtom fetches and writes both atoms', async () => {
    const store = createStore()
    expect(store.get(monthTotalUsdAtom)).toBe(null)
    expect(store.get(workspaceRollupAtom)).toEqual([])

    await store.set(refreshCostsAtom)

    expect(store.get(monthTotalUsdAtom)).toBe(42.5)
    expect(store.get(workspaceRollupAtom)).toHaveLength(2)
    expect(store.get(workspaceRollupAtom)[0].workspaceId).toBe('ws-a')
  })

  it('firedBudgetThresholdsAtom defaults to empty Set', () => {
    const store = createStore()
    const fired = store.get(firedBudgetThresholdsAtom)
    expect(fired).toBeInstanceOf(Set)
    expect(fired.size).toBe(0)
  })

  it('firedBudgetThresholdsAtom accepts adding thresholds', () => {
    const store = createStore()
    store.set(firedBudgetThresholdsAtom, new Set([80 as const]))
    expect(store.get(firedBudgetThresholdsAtom).has(80)).toBe(true)
    expect(store.get(firedBudgetThresholdsAtom).has(100)).toBe(false)
  })
})

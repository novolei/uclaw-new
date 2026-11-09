import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { UsageSettings } from './UsageSettings'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

// Recharts under jsdom emits lots of warnings about ResizeObserver
// and SVG layout — mock the layout primitives but keep the chart shells
// so the component tree renders.
vi.mock('recharts', async () => {
  return {
    ResponsiveContainer: ({ children }: any) => <div data-testid="rc-container">{children}</div>,
    BarChart: ({ children }: any) => <div data-testid="rc-bar">{children}</div>,
    Bar: () => null,
    PieChart: ({ children }: any) => <div data-testid="rc-pie">{children}</div>,
    Pie: () => null,
    Cell: () => null,
    XAxis: () => null, YAxis: () => null,
    CartesianGrid: () => null, Tooltip: () => null, Legend: () => null,
  }
})

vi.mock('@/lib/tauri-bridge', () => ({
  getDailyCosts: vi.fn(async () => [
    { day: '2026-05-08', inputTokens: 1000, outputTokens: 500, costUsd: 0.0012, turnCount: 4,
      cachedInputTokens: 600, promptTokensTotal: 1000, hitRatio: 0.6 },
    { day: '2026-05-09', inputTokens: 2000, outputTokens: 800, costUsd: 0.0024, turnCount: 6,
      cachedInputTokens: 1400, promptTokensTotal: 2000, hitRatio: 0.7 },
  ]),
  getModelCosts: vi.fn(async () => [
    { model: 'claude-4', inputTokens: 2500, outputTokens: 1100, costUsd: 0.030, turnCount: 8,
      cachedInputTokens: 2000, promptTokensTotal: 2500, hitRatio: 0.8 },
    { model: 'gpt-4o',   inputTokens: 500,  outputTokens: 200,  costUsd: 0.006, turnCount: 5,
      cachedInputTokens: 0, promptTokensTotal: 500, hitRatio: 0 },
    // Cold cache: 2 calls, nothing read yet → 0%, not 未上报.
    { model: 'deepseek', inputTokens: 300,  outputTokens: 100,  costUsd: 0.001, turnCount: 2,
      cachedInputTokens: 0, promptTokensTotal: 300, hitRatio: 0 },
  ]),
  getSessionCosts: vi.fn(async () => [
    { sessionId: 's1', title: 'Foo', inputTokens: 1500, outputTokens: 600, costUsd: 0.020, turnCount: 5, lastUsedAt: 1715000000000 },
  ]),
  onTurnCost: vi.fn(async () => () => {}),
  getMonthCostTotal: vi.fn(async () => 0.0036),
  listWorkspaceCostRollup: vi.fn(async () => []),
  getSettings: vi.fn(async () => ({ monthlyBudgetUsd: null })),
  patchSettings: vi.fn(async () => ({})),
}))

describe('UsageSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders the KPI totals from the daily rollup', async () => {
    renderWithProviders(<UsageSettings />)
    // Total cost = 0.0012 + 0.0024 = 0.0036 → "$0.0036" (sub-cent: 4 decimals)
    // May appear in both BudgetHeader and KPI card — use getAllByText
    await waitFor(() => {
      expect(screen.getAllByText(/\$0\.0036/).length).toBeGreaterThanOrEqual(1)
    })
    // Total turns = 4 + 6 = 10
    expect(screen.getByText('10')).toBeInTheDocument()
  })

  it('renders the bar + donut chart shells with data', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByTestId('rc-bar')).toBeInTheDocument()
      expect(screen.getByTestId('rc-pie')).toBeInTheDocument()
    })
  })

  it('renders the per-session table row', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByText('Foo')).toBeInTheDocument()
      expect(screen.getByText('$0.02')).toBeInTheDocument()
    })
  })

  it('renders empty-state when all rollups are empty', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.getDailyCosts).mockResolvedValueOnce([])
    vi.mocked(bridge.getModelCosts).mockResolvedValueOnce([])
    vi.mocked(bridge.getSessionCosts).mockResolvedValueOnce([])
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      // Three "暂无数据" placeholders (daily / model / session).
      expect(screen.getAllByText('暂无数据').length).toBeGreaterThanOrEqual(3)
    })
  })

  it('shows a 缓存命中 column on the daily and model tables', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getAllByRole('columnheader', { name: '缓存命中' }).length).toBe(2)
    })
    // Model row hit ratio; daily rows 60% / 70%.
    expect(screen.getByText('80%')).toBeInTheDocument()
    expect(screen.getByText('60%')).toBeInTheDocument()
    expect(screen.getByText('70%')).toBeInTheDocument()
    // 30-day KPI: (600 + 1400) / (1000 + 2000) = 67%.
    expect(screen.getByText('67%')).toBeInTheDocument()
  })

  it('marks models that never report cache reads as 未上报', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByText('未上报')).toBeInTheDocument()
    })
    // Only gpt-4o (5 calls) is flagged; deepseek (2 calls) shows 0%.
    expect(screen.getAllByText('未上报').length).toBe(1)
    expect(screen.getByText('0%')).toBeInTheDocument()
  })

  it('surfaces a load error instead of failing silently', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.getDailyCosts).mockRejectedValueOnce('db row unreadable')
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByText(/用量数据加载失败/)).toBeInTheDocument()
    })
    expect(screen.getByText(/db row unreadable/)).toBeInTheDocument()
  })
})

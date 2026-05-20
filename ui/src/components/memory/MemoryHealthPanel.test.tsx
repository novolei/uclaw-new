import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { MemoryHealthPanel } from './MemoryHealthPanel'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}))

function routeInvoke(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      memory_health_list_findings: [],
      memory_drift_list_events: [
        { id: 'e1', nodeId: 'n1', title: 'Drifting Page', score: 0.82, computedAt: 1715000000000 },
      ],
      memory_importance_list_candidates: [
        { nodeId: 'n9', title: 'Fading Note', importance: 0.07, archivePendingSince: 1714000000000, lastComputedAt: 1715000000000 },
      ],
      memory_drift_resolve_event: undefined,
      ...overrides,
    }
    const v = table[cmd]
    if (v instanceof Error) return Promise.reject(v)
    return Promise.resolve(v)
  })
}

describe('MemoryHealthPanel — E sections', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    routeInvoke()
  })

  it('renders the drift section with data + importance section title', async () => {
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText('Drifting Page')).toBeInTheDocument()
    expect(screen.getByText(/drift 0.82/)).toBeInTheDocument()
    expect(screen.getByText('重要度 · 衰减候选')).toBeInTheDocument()
  })

  it('resolves a drift event and removes the row', async () => {
    const { user } = renderWithProviders(<MemoryHealthPanel />)
    await screen.findByText('Drifting Page')
    await user.click(screen.getByTestId('health-drift-resolve'))
    await waitFor(() => expect(screen.queryByText('Drifting Page')).not.toBeInTheDocument())
    expect(invokeMock).toHaveBeenCalledWith('memory_drift_resolve_event', { input: { eventId: 'e1' } })
  })

  it('shows inline error when the drift command fails', async () => {
    routeInvoke({ memory_drift_list_events: new Error('boom') })
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText(/加载漂移失败/)).toBeInTheDocument()
  })

  it('shows empty-state text when drift has no rows', async () => {
    routeInvoke({ memory_drift_list_events: [] })
    renderWithProviders(<MemoryHealthPanel />)
    expect(await screen.findByText('无漂移事件')).toBeInTheDocument()
  })
})

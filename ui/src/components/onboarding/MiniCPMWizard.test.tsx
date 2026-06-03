import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { minicpmWizardAtom, type MiniCpmWizardState } from '@/atoms/minicpm-wizard'

// Mock before any imports that reference these modules.
// All factories are self-contained (no external var references) to satisfy vi.mock hoisting.
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
vi.mock('@/lib/tauri-bridge', () => ({
  localModelEnvCheck: vi.fn().mockResolvedValue({
    os: 'macos', arch: 'aarch64', total_ram: 16e9, free_disk: 50e9,
    metal_available: true, cpu_cores: 8, recommended_quant: 'Q4_K_M', warnings: [],
  }),
  localModelProbeSources: vi.fn().mockResolvedValue([
    { host: 'huggingface', reachable: true, latency_ms: 120 },
  ]),
  localModelDownload: vi.fn().mockResolvedValue(undefined),
  localModelCancel: vi.fn().mockResolvedValue(undefined),
  localModelWarmup: vi.fn().mockResolvedValue(undefined),
  localModelSmokeTest: vi.fn().mockResolvedValue('你好！'),
  setRoleModel: vi.fn().mockResolvedValue(undefined),
  setOnboardingState: vi.fn().mockResolvedValue(undefined),
}))

// Import the component and the mocked bridge AFTER vi.mock declarations.
import { MiniCPMWizard } from './MiniCPMWizard'
import * as bridge from '@/lib/tauri-bridge'

// Typed aliases for the mocked fns — resolved after module graph is set up.
const setRoleModel = vi.mocked(bridge.setRoleModel)
const setOnboardingState = vi.mocked(bridge.setOnboardingState)

beforeEach(() => { setRoleModel.mockClear(); setOnboardingState.mockClear() })

function renderAtStep(
  step: MiniCpmWizardState['step'],
  overrides: Partial<MiniCpmWizardState> = {},
) {
  const store = createStore()
  store.set(minicpmWizardAtom, {
    step,
    env: null,
    sources: null,
    chosenSource: null,
    progress: null,
    smokeOutput: null,
    error: null,
    ...overrides,
  })
  return render(<Provider store={store}><MiniCPMWizard /></Provider>)
}

describe('MiniCPMWizard', () => {
  it('returns null when step is null', () => {
    const store = createStore()
    const { container } = render(<Provider store={store}><MiniCPMWizard /></Provider>)
    expect(container.querySelector('[data-testid="minicpm-wizard"]')).toBeNull()
  })

  it('intro → 不再提示 persists skipped', async () => {
    renderAtStep('intro')
    fireEvent.click(screen.getByText('不再提示'))
    await waitFor(() => expect(setOnboardingState).toHaveBeenCalledWith('skipped'))
  })

  it('intro → 稍后 persists deferred', async () => {
    renderAtStep('intro')
    fireEvent.click(screen.getByText('稍后'))
    await waitFor(() => expect(setOnboardingState).toHaveBeenCalledWith('deferred'))
  })

  it('full happy path from source wires both roles + completion', async () => {
    // Seed sources so the 下载 button renders immediately (no async spinner wait).
    renderAtStep('source', {
      sources: [{ host: 'huggingface', reachable: true, latency_ms: 120 }],
      chosenSource: null,
    })
    // 下载 button visible immediately because sources is already populated.
    await waitFor(() => screen.getByText('下载 →'))
    fireEvent.click(screen.getByText('下载 →'))
    // download → warmup → smoketest → done → finish() chain resolves via mocked bridge.
    await waitFor(() => expect(setRoleModel).toHaveBeenCalledWith('utility', 'local/minicpm5-1b'))
    expect(setRoleModel).toHaveBeenCalledWith('summarizer', 'local/minicpm5-1b')
    await waitFor(() => expect(setOnboardingState).toHaveBeenCalledWith('completed'))
  })
})

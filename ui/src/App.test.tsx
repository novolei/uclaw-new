import { act } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import App, {
  STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS,
  STARTUP_SPLASH_EXIT_TRANSITION_MS,
  STARTUP_SPLASH_MIN_VISIBLE_MS,
} from './App'
import * as bridge from './lib/tauri-bridge'
import type { StartupRuntimePackStatusReport } from './lib/startup/startup-doctor'

vi.mock('./lib/tauri-bridge', () => ({
  getSettings: vi.fn(),
  getActiveModel: vi.fn(),
  getBrowserRuntimeStatus: vi.fn(),
}))

vi.mock('./components/app-shell/AppShell', () => ({
  AppShell: () => <main aria-label="App shell">App shell ready</main>,
}))

vi.mock('./hooks/useGlobalChatListeners', () => ({
  useGlobalChatListeners: vi.fn(),
}))

vi.mock('./hooks/useGlobalAgentListeners', () => ({
  useGlobalAgentListeners: vi.fn(),
}))

vi.mock('./hooks/usePetStateSync', () => ({
  usePetStateSync: vi.fn(),
}))

const getSettings = vi.mocked(bridge.getSettings)
const getActiveModel = vi.mocked(bridge.getActiveModel)
const getBrowserRuntimeStatus = vi.mocked(bridge.getBrowserRuntimeStatus)

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

const settings = {
  language: 'zh-CN',
  theme: 'system',
  theme_style: 'default',
  safety_mode: 'yolo',
}

function runtimeReport(overrides: Partial<StartupRuntimePackStatusReport> = {}): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: ['browser.runtime.doctor.completed'],
    supervisorEventNames: ['browser.startup_doctor.check'],
    supervisor: {
      providerId: 'browser.local_chromium',
      selectedSessionId: 'startup',
      runtimeState: 'stopped',
      doctorStatus: 'deferred',
      activeContextCount: 0,
      activeContextSessions: [],
    },
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: true,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Browser runtime is ready.',
      eventNames: ['browser.runtime.keep_current.planned'],
    },
    ...overrides,
  }
}

describe('App startup route', () => {
  beforeEach(() => {
    localStorage.clear()
    getSettings.mockReset()
    getActiveModel.mockReset().mockResolvedValue(null)
    getBrowserRuntimeStatus.mockReset().mockResolvedValue(runtimeReport())
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders the branded startup splash while initialization is pending', () => {
    const pendingSettings = deferred<typeof settings>()
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getSettings.mockReturnValue(pendingSettings.promise)
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()
    expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(1)
  })

  it('keeps the splash visible for a perceptible minimum before AppShell handoff', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(getActiveModel).toHaveBeenCalledTimes(1)
    expect(localStorage.getItem('uclaw:language')).toBe('zh-CN')
    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS - 1)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
  })

  it('waits for Rust Browser Runtime status before AppShell handoff', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS + STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      pendingRuntime.resolve(runtimeReport())
      await Promise.resolve()
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
  })

  it('records a bounded fallback when Rust Browser Runtime status fails', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    getBrowserRuntimeStatus.mockRejectedValue(new Error('runtime status unavailable'))
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(screen.getAllByText(/Rust Browser Runtime status is unavailable/).length).toBeGreaterThan(0)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(consoleError).toHaveBeenCalledWith(
      '[App] Browser Runtime 状态读取失败:',
      expect.any(Error),
    )
  })

  it('records a bounded fallback when Rust Browser Runtime status hangs', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS)
    })

    expect(screen.getAllByText(/did not respond within/).length).toBeGreaterThan(0)
    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(consoleError).toHaveBeenCalledWith(
      '[App] Browser Runtime 状态读取失败:',
      expect.any(Error),
    )
  })
})

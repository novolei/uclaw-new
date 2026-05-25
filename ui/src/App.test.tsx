import { act } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import App, {
  STARTUP_SPLASH_EXIT_TRANSITION_MS,
  STARTUP_SPLASH_MIN_VISIBLE_MS,
} from './App'
import * as bridge from './lib/tauri-bridge'

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

describe('App startup route', () => {
  beforeEach(() => {
    localStorage.clear()
    getSettings.mockReset()
    getActiveModel.mockReset().mockResolvedValue(null)
    getBrowserRuntimeStatus.mockReset().mockRejectedValue(new Error('runtime status unavailable'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders the branded startup splash while initialization is pending', () => {
    const pendingSettings = deferred<typeof settings>()
    getSettings.mockReturnValue(pendingSettings.promise)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()
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

    expect(screen.getByText('Preparing uClaw').closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
  })
})

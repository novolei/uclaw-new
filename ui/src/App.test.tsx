import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import App from './App'
import * as bridge from './lib/tauri-bridge'

vi.mock('./lib/tauri-bridge', () => ({
  getSettings: vi.fn(),
  getActiveModel: vi.fn(),
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
  })

  it('renders the branded startup splash while initialization is pending', () => {
    const pendingSettings = deferred<typeof settings>()
    getSettings.mockReturnValue(pendingSettings.promise)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()
  })

  it('hands off to AppShell after existing initialization completes', async () => {
    getSettings.mockResolvedValue(settings)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()

    await waitFor(() => {
      expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
    })

    expect(localStorage.getItem('uclaw:language')).toBe('zh-CN')
    expect(getActiveModel).toHaveBeenCalledTimes(1)
  })
})

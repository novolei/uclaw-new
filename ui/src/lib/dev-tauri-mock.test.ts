import { afterEach, describe, expect, it, vi } from 'vitest'
import { clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen } from '@tauri-apps/api/event'
import {
  createUclawMockIpcHandler,
  installDevTauriMock,
  shouldInstallDevTauriMock,
} from './dev-tauri-mock'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>
    __UCLAW_DEV_TAURI_MOCK__?: boolean
  }
}

afterEach(() => {
  vi.unstubAllEnvs()
  clearMocks()
  delete window.__UCLAW_DEV_TAURI_MOCK__
})

describe('dev tauri mock', () => {
  it('stays disabled unless explicitly requested', () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', undefined)
    expect(shouldInstallDevTauriMock()).toBe(false)
  })

  it('stays disabled inside a real Tauri runtime', () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    window.__TAURI_INTERNALS__ = { invoke: async () => null }
    expect(shouldInstallDevTauriMock()).toBe(false)
  })

  it('installs official Tauri mocks and returns startup fixtures', async () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    installDevTauriMock()

    await expect(invoke('get_settings')).resolves.toMatchObject({
      language: 'zh-CN',
      theme: 'system',
    })
    await expect(invoke('get_active_model')).resolves.toBeNull()
    expect(window.__UCLAW_DEV_TAURI_MOCK__).toBe(true)
  })

  it('supports event listen and emit for browser-only interaction checks', async () => {
    vi.stubEnv('VITE_UCLAW_MOCK_TAURI', '1')
    installDevTauriMock()
    const handler = vi.fn()

    const unlisten = await listen('automation://activity', handler)
    await emit('automation://activity', { id: 'activity-1' })
    await unlisten()

    expect(handler).toHaveBeenCalledWith(expect.objectContaining({
      event: 'automation://activity',
      payload: { id: 'activity-1' },
    }))
  })

  it('returns a visible diagnostics fixture for SystemTab', async () => {
    const handler = createUclawMockIpcHandler()
    const result = await handler('get_system_diagnostics')

    expect(result).toMatchObject({
      app_version: 'dev-mock',
      platform: 'browser',
      memu: { running: true },
      gbrain: { connected: true, tool_count: 6 },
    })
  })
})

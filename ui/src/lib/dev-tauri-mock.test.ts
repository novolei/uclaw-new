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
    await expect(invoke('get_user_profile')).resolves.toMatchObject({
      userName: 'Mock User',
    })
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

  it('returns Browser Runtime Control Center fixtures for browser-only validation', async () => {
    const handler = createUclawMockIpcHandler()

    await expect(await handler('list_agent_sessions')).toEqual([])
    await expect(await handler('get_browser_runtime_status')).toMatchObject({
      controlCenter: {
        desiredProviderPriority: [
          'browser.playwright_cli',
          'browser.playwright_mcp',
          'browser.local_chromium',
        ],
        activeProviderRoute: { providerId: 'browser.local_chromium' },
      },
    })
    await expect(await handler('get_browser_runtime_control_center')).toMatchObject({
      mcpIntegrationSummary: {
        builtIn: true,
        rawToolsExposed: false,
      },
    })
    await expect(await handler('get_daily_costs')).toEqual([])
    await expect(await handler('list_workspace_cost_rollup')).toEqual([])
    await expect(await handler('get_month_cost_total')).toBe(0)
    await expect(await handler('list_providers')).toEqual([])
    await expect(await handler('get_all_configured_models')).toEqual([])
    await expect(await handler('list_mcp_tools')).toEqual([])
    await expect(await handler('list_automations')).toEqual([])
  })

  it('mocks confirmed Browser Runtime execute IPC for browser-only validation', async () => {
    const handler = createUclawMockIpcHandler()

    await expect(await handler('execute_browser_runtime_action', {
      action: 'prepare',
      confirmed: true,
    })).toMatchObject({
      operation: 'prepare',
      mode: 'managed',
      status: 'succeeded',
      sourceKind: 'dev_staging',
      eventNames: ['browser.runtime.mock.managed_succeeded'],
    })
  })
})

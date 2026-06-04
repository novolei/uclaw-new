import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { PluginsSettings } from './PluginsSettings'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

const PLUGIN = { id: 'p1', display_name: 'Hello Plugin', version: '0.1.0', enabled: true, mcp_connected: true }
const PLUGIN_DETAIL = {
  id: 'p1', display_name: 'Hello Plugin', version: '0.1.0', enabled: true, mcp_connected: true,
  author_name: 'alice', contributes: {}, permissions: { network: false, filesystem_read: false, filesystem_write: false, memory_read: false, memory_write: false, run_subprocess: false },
}

beforeEach(() => { invokeMock.mockReset() })

describe('PluginsSettings', () => {
  it('lists plugins from list_plugins', async () => {
    invokeMock.mockImplementation((cmd: string) => cmd === 'list_plugins' ? Promise.resolve([PLUGIN]) : Promise.resolve())
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('Hello Plugin')).toBeInTheDocument())
    expect(screen.getByText(/0\.1\.0/)).toBeInTheDocument()
  })

  it('toggles via set_plugin_enabled (toggle is in detail drawer)', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'list_plugins') return Promise.resolve([PLUGIN])
      if (cmd === 'get_plugin_detail') return Promise.resolve(PLUGIN_DETAIL)
      return Promise.resolve()
    })
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('Hello Plugin')).toBeInTheDocument())
    // Open the detail drawer by clicking the card
    fireEvent.click(screen.getByText('Hello Plugin'))
    // The drawer's Switch is now rendered (aria-label="启用")
    const sw = await waitFor(() => screen.getByRole('switch', { name: '启用' }))
    fireEvent.click(sw)
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('set_plugin_enabled', { id: 'p1', enabled: false }))
  })

  it('shows empty state when no plugins', async () => {
    invokeMock.mockImplementation((cmd: string) => cmd === 'list_plugins' ? Promise.resolve([]) : Promise.resolve())
    renderWithProviders(<PluginsSettings />)
    await waitFor(() => expect(screen.getByText('未安装插件')).toBeInTheDocument())
  })
})

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { imChannelsAtom, imChannelStatusesAtom } from '@/atoms/im-channel-atoms'
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelsSettings } from './ImChannelsSettings'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

const makeChannel = (overrides: Partial<ImChannelRow> = {}): ImChannelRow => ({
  id: 'ch-1', spaceId: 'sp-1', channelType: 'wecom_bot', name: '产品组机器人',
  config: { corp_id: 'wx12abc', agent_id: '1000042' }, enabled: true,
  streaming: false, replyScope: 'all', permissionEnabled: false,
  owners: [], guestPolicy: { tool_allowlist: [], mcp_enabled: false },
  createdAt: 1_700_000_000_000, updatedAt: 1_700_000_000_000,
  ...overrides,
})

beforeEach(() => {
  invokeMock.mockReset()
  // Default: list_im_channels, get_im_channel_statuses, list_spaces all return empty
  invokeMock.mockResolvedValue([])
})

describe('ImChannelsSettings', () => {
  it('renders tab with instance count badge', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('企业微信')).not.toBeNull()
    expect(screen.getByText('1')).not.toBeNull()
  })

  it('shows error badge on tab when any instance has error status', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ id: 'ch-err' })])
    store.set(imChannelStatusesAtom, {
      'ch-err': { instanceId: 'ch-err', state: 'error', lastError: '认证失败' } as ImChannelStatus,
    })
    renderWithProviders(<ImChannelsSettings />, { store })
    const badge = screen.getByText('1')
    expect(badge.className).toMatch(/destructive/)
  })

  it('renders instance name in the list', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ name: '测试机器人' })])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('测试机器人')).not.toBeNull()
  })

  it('renders add-new dashed button for current tab', () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText(/新增企业微信实例/)).not.toBeNull()
  })

  it('calls toggle_im_channel and optimistically updates enabled state', async () => {
    invokeMock.mockResolvedValue(undefined)
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ enabled: true })])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('toggle_im_channel', { id: 'ch-1', enabled: false })
    })
  })

  it('reverts optimistic toggle on invoke failure', async () => {
    invokeMock.mockRejectedValue(new Error('network error'))
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ enabled: true })])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('list_im_channels')
    })
  })
})

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ImChannelStatus } from '@/atoms/im-channel-atoms'
import { WechatIlinkBindingPanel } from './WechatIlinkBindingPanel'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))
vi.mock('qrcode', () => ({
  default: { toCanvas: vi.fn().mockResolvedValue(undefined) },
}))

const PROPS = {
  instanceId: 'inst-1',
  onSaved: vi.fn(),
  onDisconnect: vi.fn(),
}

beforeEach(() => {
  invokeMock.mockReset()
  PROPS.onSaved = vi.fn()
  PROPS.onDisconnect = vi.fn()
})

describe('WechatIlinkBindingPanel', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('idle: shows get-qr button, no canvas', () => {
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    expect(screen.getByText('获取二维码')).not.toBeNull()
    expect(screen.queryByRole('img')).toBeNull()
  })

  it('qr-shown: fetching QR invokes request command and shows canvas', async () => {
    invokeMock.mockResolvedValueOnce({ qrcode: 'mock_qr_data' })
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    const btn = screen.getByText('获取二维码')
    await act(async () => { btn.click() })
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('request_wechat_ilink_qrcode', { instanceId: 'inst-1' })
    )
    expect(screen.getByText('用微信扫码绑定账号')).not.toBeNull()
  })

  it('scanning: poll returning scaned shows "已扫码" text', async () => {
    vi.useFakeTimers()
    invokeMock
      .mockResolvedValueOnce({ qrcode: 'qr123' })               // request_wechat_ilink_qrcode
      .mockResolvedValueOnce({ status: 'wait' })                 // first poll
      .mockResolvedValueOnce({ status: 'scaned' })               // second poll → scanning
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    // fire first interval tick and drain all resulting promises
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    // fire second interval tick and drain all resulting promises
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    expect(screen.getByText('已扫码，等待确认…')).not.toBeNull()
  })

  it('confirmed: poll returning confirmed calls save_wechat_ilink_token and onSaved', async () => {
    vi.useFakeTimers()
    invokeMock
      .mockResolvedValueOnce({ qrcode: 'qr123' })
      .mockResolvedValueOnce({ status: 'confirmed', bot_token: 'tok999', account_id: 'acc456' })
      .mockResolvedValueOnce(undefined)                          // save_wechat_ilink_token
    renderWithProviders(
      <WechatIlinkBindingPanel {...PROPS} status={undefined} />
    )
    await act(async () => { screen.getByText('获取二维码').click() })
    // fire first interval tick and drain all resulting promises (including saveToken)
    await act(async () => { await vi.advanceTimersByTimeAsync(2100) })
    expect(invokeMock).toHaveBeenCalledWith(
      'save_wechat_ilink_token',
      expect.objectContaining({ instanceId: 'inst-1', botToken: 'tok999', accountId: 'acc456' })
    )
    expect(PROPS.onSaved).toHaveBeenCalledOnce()
  })
})

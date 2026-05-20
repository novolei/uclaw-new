import { describe, it, expect, vi, beforeEach } from 'vitest'
import { openAutomationLoginWindow } from './automation-login-window'

const mocks = vi.hoisted(() => {
  const getByLabel = vi.fn()
  const focus = vi.fn()
  const WebviewWindowCtor = vi.fn()
  return { getByLabel, focus, WebviewWindowCtor }
})

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: Object.assign(mocks.WebviewWindowCtor, {
    getByLabel: mocks.getByLabel,
  }),
}))

describe('openAutomationLoginWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.getByLabel.mockResolvedValue(null)
    mocks.focus.mockResolvedValue(undefined)
  })

  it('creates a dedicated login window routed to the browser shell with encoded target url', async () => {
    await openAutomationLoginWindow({
      specId: 'builtin://automation-specs/bilibili-comment-auto-reply',
      label: 'Bilibili',
      url: 'https://www.bilibili.com',
    })

    expect(mocks.WebviewWindowCtor).toHaveBeenCalledWith(
      expect.stringMatching(/^automation-login-/),
      expect.objectContaining({
        title: 'Bilibili 登录',
        url: expect.stringContaining('/?uclawWindow=automation-login-browser'),
        width: 1180,
        height: 820,
      }),
    )
    expect(mocks.WebviewWindowCtor.mock.calls[0][1].url).toContain(
      'targetUrl=https%3A%2F%2Fwww.bilibili.com',
    )
  })

  it('focuses the existing dedicated window instead of opening an in-app tab', async () => {
    mocks.getByLabel.mockResolvedValue({ focus: mocks.focus })

    await openAutomationLoginWindow({
      specId: 'spec-1',
      label: 'Bilibili',
      url: 'https://www.bilibili.com',
    })

    expect(mocks.focus).toHaveBeenCalled()
    expect(mocks.WebviewWindowCtor).not.toHaveBeenCalled()
  })
})

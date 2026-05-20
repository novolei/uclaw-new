import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { openAutomationLoginWindow } from './automation-login-window'

const mocks = vi.hoisted(() => {
  const getByLabel = vi.fn()
  const focus = vi.fn()
  const WebviewWindowCtor = vi.fn()
  const browserWebviewCompleteLogin = vi.fn()
  return { getByLabel, focus, WebviewWindowCtor, browserWebviewCompleteLogin }
})

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: Object.assign(mocks.WebviewWindowCtor, {
    getByLabel: mocks.getByLabel,
  }),
}))

vi.mock('./tauri-bridge', () => ({
  browserWebviewCompleteLogin: mocks.browserWebviewCompleteLogin,
}))

describe('openAutomationLoginWindow', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    mocks.getByLabel.mockResolvedValue(null)
    mocks.focus.mockResolvedValue(undefined)
    mocks.browserWebviewCompleteLogin.mockResolvedValue({ completed: false })
  })

  afterEach(() => {
    vi.clearAllTimers()
    vi.useRealTimers()
  })

  it('creates a dedicated login window that opens the target site directly', async () => {
    await openAutomationLoginWindow({
      specId: 'builtin://automation-specs/bilibili-comment-auto-reply',
      label: 'Bilibili',
      url: 'https://www.bilibili.com',
    })

    expect(mocks.WebviewWindowCtor).toHaveBeenCalledWith(
      expect.stringMatching(/^automation-login-/),
      expect.objectContaining({
        title: 'Bilibili 登录',
        url: 'https://www.bilibili.com',
        width: 1180,
        height: 820,
      }),
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

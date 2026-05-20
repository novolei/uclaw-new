import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, waitFor } from '@testing-library/react'
import { BrowserPanel } from './BrowserPanel'
import { browserUINavigate } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  listenNavState: vi.fn(() => Promise.resolve(() => {})),
  browserGetDOMState: vi.fn(),
  browserStartScreencast: vi.fn().mockResolvedValue(undefined),
  browserStopScreencast: vi.fn().mockResolvedValue(undefined),
  browserUINavigate: vi.fn().mockResolvedValue('tab-1'),
}))

vi.mock('@/hooks/useBrowserScreencast', () => ({
  useBrowserScreencast: vi.fn(),
}))

describe('BrowserPanel login initial navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('navigates a fresh browser session to the provided initialUrl', async () => {
    render(<BrowserPanel agentSessionId="automation-login:spec-1" initialUrl="https://www.bilibili.com" />)

    await waitFor(() => {
      expect(browserUINavigate).toHaveBeenCalledWith(
        'automation-login:spec-1',
        'new',
        'https://www.bilibili.com',
      )
    })
  })
})

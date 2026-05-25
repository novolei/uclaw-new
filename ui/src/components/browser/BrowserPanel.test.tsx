import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, waitFor } from '@testing-library/react'
import { BrowserPanel } from './BrowserPanel'
import { browserUINavigate, getBrowserRuntimeStatus } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  getBrowserRuntimeStatus: vi.fn().mockResolvedValue({
    manifestPackVersion: '1.48.2-uclaw.1',
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Runtime ready',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
      activeTasks: 0,
    },
    primaryAction: 'keep_current',
    operationPlan: { status: 'ready', summary: 'Runtime ready' },
    ready: true,
    canRunBrowserTasks: true,
    eventNames: [],
    supervisor: {
      providerId: 'local_chromium',
      selectedSessionId: 'startup',
      runtimeState: 'ready',
      doctorStatus: 'ready',
      activeContextCount: 0,
      activeContextSessions: [],
    },
  }),
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
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValue({
      manifestPackVersion: '1.48.2-uclaw.1',
      doctor: {
        status: 'ready',
        ready: true,
        remediation: 'Runtime ready',
        actions: ['keep_current'],
        manifestPackVersion: '1.48.2-uclaw.1',
        rollbackAvailable: false,
        activeTasks: 0,
      },
      primaryAction: 'keep_current',
      operationPlan: { status: 'ready', summary: 'Runtime ready' },
      ready: true,
      canRunBrowserTasks: true,
      eventNames: [],
      supervisor: {
        providerId: 'local_chromium',
        selectedSessionId: 'startup',
        runtimeState: 'ready',
        doctorStatus: 'ready',
        activeContextCount: 0,
        activeContextSessions: [],
      },
    })
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

  it('loads Rust browser runtime status for the panel status bar', async () => {
    render(<BrowserPanel agentSessionId="agent-session-1" />)

    await waitFor(() => {
      expect(getBrowserRuntimeStatus).toHaveBeenCalled()
    })
  })
})

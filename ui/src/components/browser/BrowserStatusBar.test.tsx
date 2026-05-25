import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { BrowserStatusBar } from './BrowserStatusBar'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'

function runtimeReport(
  overrides: Partial<StartupRuntimePackStatusReport> = {},
): StartupRuntimePackStatusReport {
  return {
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
      selectedSessionId: 'agent-session-1',
      runtimeState: 'ready',
      doctorStatus: 'ready',
      activeContextCount: 1,
      activeContextSessions: ['agent-session-1'],
    },
    ...overrides,
  }
}

describe('BrowserStatusBar runtime projection', () => {
  it('renders the Rust supervisor runtime state', () => {
    render(<BrowserStatusBar sessionId="agent-session-1" runtimeStatus={runtimeReport()} />)

    expect(screen.getByText('Runtime ready')).toBeInTheDocument()
    expect(screen.getByTitle(/provider=local_chromium/)).toBeInTheDocument()
  })

  it('renders runtime status errors without hiding browser controls', () => {
    render(
      <BrowserStatusBar
        sessionId="agent-session-1"
        runtimeStatusError="runtime bridge unavailable"
      />,
    )

    expect(screen.getByText('Runtime unavailable')).toBeInTheDocument()
    expect(screen.getByTitle('runtime bridge unavailable')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '标注' })).toBeInTheDocument()
  })
})

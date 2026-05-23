import { describe, expect, it } from 'vitest'
import { screen } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { BrowserRuntimeSettings } from './BrowserRuntimeSettings'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'

function runtimeReport(): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: ['browser.runtime.doctor.completed'],
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: true,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Runtime pack is ready.',
      eventNames: ['browser.runtime.keep_current.ready'],
    },
  }
}

describe('BrowserRuntimeSettings', () => {
  it('renders a readonly default surface before IPC wiring lands', () => {
    renderWithProviders(<BrowserRuntimeSettings />)

    expect(screen.getByText('浏览器运行时')).toBeInTheDocument()
    expect(screen.getAllByText('未检查').length).toBeGreaterThan(1)
    expect(screen.getByText('等待运行时状态')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '准备' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '运行诊断' })).toBeDisabled()
  })

  it('renders runtime metadata from the Phase 2 status report adapter', () => {
    renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
          artifactSizeBytes: 734 * 1024 * 1024,
          runtimePackPath: '/uclaw/browser-runtime/v1',
          releaseChannel: 'stable',
          updateState: 'current',
          developerFallbackEnabled: false,
          autoPrepareEnabled: true,
        }}
      />,
    )

    expect(screen.getAllByText('可用').length).toBeGreaterThan(1)
    expect(screen.getByText('1.48.2-uclaw.1')).toBeInTheDocument()
    expect(screen.getByText('734 MiB')).toBeInTheDocument()
    expect(screen.getByText('/uclaw/browser-runtime/v1')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '保持当前' })).toBeDisabled()
  })
})

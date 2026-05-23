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
    expect(screen.getAllByText('等待运行时状态').length).toBeGreaterThan(1)
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
    expect(screen.getByRole('button', { name: '保持当前' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '关闭自动准备' })).toBeEnabled()
    expect(screen.getByText('操作预览')).toBeInTheDocument()
    expect(screen.getByText('browser.runtime.keep_current.ready · browser.runtime.doctor.completed')).toBeInTheDocument()
  })

  it('selects runtime action intents without invoking side effects', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: {
            ...runtimeReport(),
            ready: false,
            canRunBrowserTasks: false,
            primaryAction: 'repair',
            doctor: {
              status: 'needs_repair',
              ready: false,
              issue: 'corrupt_cache',
              remediation: 'Runtime cache is corrupt.',
              actions: ['repair', 'rollback'],
              manifestPackVersion: '1.48.2-uclaw.1',
              rollbackAvailable: true,
              activeTasks: 0,
            },
            operationPlan: {
              status: 'planned',
              summary: 'Repair Browser runtime pack after policy checks.',
              eventNames: ['browser.runtime.repair.planned'],
            },
          },
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '回滚' }))

    expect(screen.getByText('回滚到上一个可用 Browser runtime pack，需要明确确认并等待后端执行边界接入。')).toBeInTheDocument()
    expect(screen.getByText('需要确认')).toBeInTheDocument()
    expect(screen.getByText('无副作用')).toBeInTheDocument()
  })

  it('previews auto-prepare control without disabling browser capability', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
          autoPrepareEnabled: true,
        }}
      />,
    )

    await user.click(screen.getByRole('button', { name: '关闭自动准备' }))

    expect(screen.getByText('关闭启动/后台自动准备；浏览器任务仍可在使用时请求准备运行时。')).toBeInTheDocument()
    expect(screen.getByText('browser.runtime.auto_prepare.disable.requested')).toBeInTheDocument()
    expect(screen.getByText('预览')).toBeInTheDocument()
  })
})

import { describe, expect, it, vi } from 'vitest'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'
import { renderWithProviders, screen, within } from '@/test-utils/render'
import { deriveBrowserRuntimeTaskTimePrompt } from '@/lib/browser-runtime/browser-runtime-task-prompt'
import { BrowserRuntimeTaskTimePrompt } from './BrowserRuntimeTaskTimePrompt'

function runtimeReport(
  overrides: Partial<StartupRuntimePackStatusReport> = {},
): StartupRuntimePackStatusReport {
  const base: StartupRuntimePackStatusReport = {
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

  return {
    ...base,
    ...overrides,
    doctor: {
      ...base.doctor,
      ...overrides.doctor,
    },
    operationPlan: {
      ...base.operationPlan,
      ...overrides.operationPlan,
    },
  }
}

describe('BrowserRuntimeTaskTimePrompt', () => {
  it('renders nothing when runtime is already ready', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport(),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
      taskLabel: '网页采集',
    })

    const { container } = renderWithProviders(<BrowserRuntimeTaskTimePrompt model={model} />)

    expect(container.firstChild).toBeNull()
  })

  it('renders prepare, checkpointed defer, and disabled no-browser actions', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'prepare',
        doctor: {
          status: 'needs_prepare',
          ready: false,
          issue: 'missing_manifest',
          remediation: 'Runtime pack is missing.',
          actions: ['prepare', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'planned',
          summary: 'Prepare Browser runtime pack.',
          eventNames: ['browser.runtime.prepare.planned'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
      taskLabel: '网页采集',
    })

    renderWithProviders(<BrowserRuntimeTaskTimePrompt model={model} />)

    const prompt = screen.getByRole('region', { name: '浏览器运行时任务提示' })
    expect(within(prompt).getByText('需要准备浏览器运行时')).toBeInTheDocument()
    expect(within(prompt).getByText('待准备')).toBeInTheDocument()
    expect(within(prompt).getByText('将暂停任务')).toBeInTheDocument()
    expect(within(prompt).getByText('checkpoint: paused_waiting_for_browser_runtime'))
      .toBeInTheDocument()

    expect(within(prompt).getByRole('button', { name: '现在准备' })).toBeEnabled()
    expect(within(prompt).getByRole('button', { name: '稍后处理' })).toBeEnabled()
    expect(within(prompt).getByRole('button', { name: '不用浏览器继续' })).toBeDisabled()
    expect(within(prompt).getByText(/browser\.runtime\.task_time\.prepare\.requested/))
      .toBeInTheDocument()
  })

  it('reports the selected local action without executing runtime side effects', async () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'prepare',
        doctor: {
          status: 'needs_prepare',
          ready: false,
          issue: 'missing_browser_binary',
          remediation: 'Browser binary is missing.',
          actions: ['prepare', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'planned',
          summary: 'Prepare Browser runtime pack.',
          eventNames: ['browser.runtime.prepare.planned'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
    })
    const onAction = vi.fn()
    const { user } = renderWithProviders(
      <BrowserRuntimeTaskTimePrompt model={model} onAction={onAction} />,
    )

    await user.click(screen.getByRole('button', { name: '现在准备' }))

    expect(onAction).toHaveBeenCalledTimes(1)
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ id: 'prepare_now' }))
  })

  it('links task-time prompt attention to Browser Runtime settings', async () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'prepare',
        doctor: {
          status: 'needs_prepare',
          ready: false,
          issue: 'missing_manifest',
          remediation: 'Runtime pack is missing.',
          actions: ['prepare', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'planned',
          summary: 'Prepare Browser runtime pack.',
          eventNames: ['browser.runtime.prepare.planned'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
    })
    const onOpenBrowserRuntimeSettings = vi.fn()
    const { user } = renderWithProviders(
      <BrowserRuntimeTaskTimePrompt
        model={model}
        onOpenBrowserRuntimeSettings={onOpenBrowserRuntimeSettings}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Browser Runtime Settings' }))

    expect(onOpenBrowserRuntimeSettings).toHaveBeenCalledTimes(1)
  })

  it('hides the Browser Runtime settings link when no callback is supplied', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'prepare',
        doctor: {
          status: 'needs_prepare',
          ready: false,
          issue: 'missing_manifest',
          remediation: 'Runtime pack is missing.',
          actions: ['prepare', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'planned',
          summary: 'Prepare Browser runtime pack.',
          eventNames: ['browser.runtime.prepare.planned'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
    })

    renderWithProviders(<BrowserRuntimeTaskTimePrompt model={model} />)

    expect(screen.queryByRole('button', { name: 'Browser Runtime Settings' })).not.toBeInTheDocument()
  })

  it('renders a no-browser fallback as the primary action when preparation is blocked', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'rollback',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'worker_startup_failure',
          remediation: 'Runtime worker failed.',
          actions: ['rollback', 'reinstall'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'blocked',
          summary: 'Rollback is blocked because no previous pack exists.',
          eventNames: ['browser.runtime.rollback.blocked'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: true,
      taskLabel: '资料整理',
    })

    renderWithProviders(<BrowserRuntimeTaskTimePrompt model={model} />)

    const prompt = screen.getByRole('region', { name: '浏览器运行时任务提示' })
    expect(within(prompt).getByText('受阻')).toBeInTheDocument()
    expect(within(prompt).getByRole('button', { name: '现在准备' })).toBeDisabled()
    expect(within(prompt).getByRole('button', { name: '不用浏览器继续' })).toBeEnabled()
    expect(within(prompt).getByText('推荐')).toBeInTheDocument()
    expect(within(prompt).queryByText('将暂停任务')).not.toBeInTheDocument()
  })
})

import { describe, expect, it } from 'vitest'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'
import { deriveBrowserRuntimeTaskTimePrompt } from './browser-runtime-task-prompt'

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

describe('browser runtime task-time prompt model', () => {
  it('does not show a prompt when browser runtime can already run tasks', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport(),
      browserRequired: true,
      noBrowserFallbackAvailable: false,
      taskLabel: '网页采集',
    })

    expect(model.shouldShowPrompt).toBe(false)
    expect(model.status).toBe('ready')
    expect(model.actions).toEqual([])
  })

  it('offers prepare now and checkpointed defer when browser is required', () => {
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

    expect(model.shouldShowPrompt).toBe(true)
    expect(model.status).toBe('prepare_required')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'prepare_now',
          enabled: true,
          primary: true,
          eventNames: [
            'browser.runtime.task_time.prepare.requested',
            'browser.runtime.prepare.planned',
            'browser.runtime.doctor.completed',
          ],
        }),
        expect.objectContaining({
          id: 'defer',
          checkpointStatus: 'paused_waiting_for_browser_runtime',
          eventNames: ['browser.runtime.task_time.defer.checkpointed'],
        }),
        expect.objectContaining({
          id: 'continue_without_browser',
          enabled: false,
        }),
      ]),
    )
  })

  it('lets tasks continue without browser when a fallback can satisfy the request', () => {
    const model = deriveBrowserRuntimeTaskTimePrompt({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'repair',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'corrupt_cache',
          remediation: 'Runtime cache is corrupt.',
          actions: ['repair', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: true,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'requires_confirmation',
          summary: 'Repair requires confirmation.',
          eventNames: ['browser.runtime.repair.confirmation_required'],
        },
      }),
      browserRequired: true,
      noBrowserFallbackAvailable: true,
      taskLabel: '资料整理',
    })

    expect(model.status).toBe('confirmation_required')
    expect(model.summary).toContain('资料整理')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'prepare_now',
          enabled: true,
          summary: 'Repair requires confirmation.',
        }),
        expect.objectContaining({
          id: 'defer',
          checkpointStatus: undefined,
          eventNames: ['browser.runtime.task_time.defer.recorded'],
        }),
        expect.objectContaining({
          id: 'continue_without_browser',
          enabled: true,
          summary: '使用无浏览器能力继续当前任务。',
        }),
      ]),
    )
  })

  it('blocks prepare now when runtime preparation is blocked', () => {
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
    })

    expect(model.status).toBe('blocked')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'prepare_now',
          enabled: false,
        }),
        expect.objectContaining({
          id: 'continue_without_browser',
          enabled: true,
          primary: true,
        }),
      ]),
    )
  })
})

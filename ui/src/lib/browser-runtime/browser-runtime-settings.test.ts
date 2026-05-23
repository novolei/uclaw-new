import { describe, expect, it } from 'vitest'
import {
  deriveBrowserRuntimeSettingsViewModel,
  type BrowserRuntimeSettingsInput,
} from './browser-runtime-settings'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'

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

describe('browser runtime settings view model', () => {
  it('surfaces ready runtime metadata without enabling side effects', () => {
    const input: BrowserRuntimeSettingsInput = {
      report: runtimeReport(),
      artifactSizeBytes: 512 * 1024 * 1024,
      runtimePackPath: '/Users/ryan/Library/Application Support/uClaw/browser-runtime/v1',
      releaseChannel: 'stable',
      updateState: 'current',
      developerFallbackEnabled: false,
      autoPrepareEnabled: true,
    }

    const model = deriveBrowserRuntimeSettingsViewModel(input)

    expect(model.statusKind).toBe('ready')
    expect(model.versionLabel).toBe('1.48.2-uclaw.1')
    expect(model.artifactSizeLabel).toBe('512 MiB')
    expect(model.rollbackLabel).toBe('可用')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'keep_current', enabled: false }),
        expect.objectContaining({ id: 'run_doctor', enabled: false }),
      ]),
    )
  })

  it('marks blocked operation plans as unavailable runtime state', () => {
    const model = deriveBrowserRuntimeSettingsViewModel({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'rollback',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'worker_startup_failure',
          remediation: 'Browser runtime worker failed.',
          actions: ['rollback', 'reinstall'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'blocked',
          summary: 'Rollback is blocked because no previous runtime pack exists.',
          eventNames: ['browser.runtime.rollback.blocked'],
        },
      }),
    })

    expect(model.statusKind).toBe('blocked')
    expect(model.statusDetail).toBe('Rollback is blocked because no previous runtime pack exists.')
    expect(model.rollbackLabel).toBe('不可用')
  })

  it('keeps the default state readonly before IPC wiring exists', () => {
    const model = deriveBrowserRuntimeSettingsViewModel()

    expect(model.statusKind).toBe('unknown')
    expect(model.versionLabel).toBe('未检查')
    expect(model.runtimePackPathLabel).toBe('等待运行时状态')
    expect(model.actions.every((action) => !action.enabled)).toBe(true)
  })
})

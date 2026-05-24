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
    runtimeRoot: '/Users/ryan/Library/Application Support/uClaw/browser-runtime',
    currentPackDir:
      '/Users/ryan/Library/Application Support/uClaw/browser-runtime/packs/browser-runtime-pack-v1',
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
    expect(model.runtimeRootLabel).toBe(
      '/Users/ryan/Library/Application Support/uClaw/browser-runtime',
    )
    expect(model.runtimePackPathLabel).toBe(
      '/Users/ryan/Library/Application Support/uClaw/browser-runtime/packs/browser-runtime-pack-v1',
    )
    expect(model.rollbackLabel).toBe('可用')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'keep_current',
          enabled: true,
          preview: expect.objectContaining({
            eventNames: [
              'browser.runtime.keep_current.ready',
              'browser.runtime.doctor.completed',
            ],
          }),
        }),
        expect.objectContaining({ id: 'run_doctor', enabled: true }),
        expect.objectContaining({
          id: 'disable_auto_prepare',
          enabled: true,
          preview: expect.objectContaining({
            eventNames: ['browser.runtime.auto_prepare.disable.requested'],
            summary: '关闭启动/后台自动准备；浏览器任务仍可在使用时请求准备运行时。',
          }),
        }),
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
    expect(model.runtimeRootLabel).toBe('等待运行时状态')
    expect(model.runtimePackPathLabel).toBe('等待运行时状态')
    expect(model.autoPrepareLabel).toBe('等待运行时状态')
    expect(model.actions.every((action) => !action.enabled)).toBe(true)
  })

  it('keeps legacy runtime pack path previews when no live report exists', () => {
    const model = deriveBrowserRuntimeSettingsViewModel({
      runtimePackPath: '/preview/browser-runtime/current',
    })

    expect(model.runtimeRootLabel).toBe('等待运行时状态')
    expect(model.runtimePackPathLabel).toBe('/preview/browser-runtime/current')
  })

  it('keeps auto-prepare disabled semantics separate from browser capability', () => {
    const model = deriveBrowserRuntimeSettingsViewModel({
      report: runtimeReport(),
      autoPrepareEnabled: false,
    })

    expect(model.autoPrepareLabel).toBe('已关闭')
    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'enable_auto_prepare',
          label: '开启自动准备',
          enabled: true,
          preview: expect.objectContaining({
            destructive: false,
            requiresConfirmation: false,
            eventNames: ['browser.runtime.auto_prepare.enable.requested'],
            summary: '恢复启动/后台自动准备；不会立即下载或修复运行时。',
          }),
        }),
      ]),
    )
  })

  it('marks destructive settings intents as confirmation-only previews', () => {
    const model = deriveBrowserRuntimeSettingsViewModel({
      report: runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'reinstall',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'corrupt_cache',
          remediation: 'Runtime cache is corrupt.',
          actions: ['cleanup', 'reinstall'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: true,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'requires_confirmation',
          summary: 'Reinstall requires explicit confirmation.',
          eventNames: ['browser.runtime.reinstall.confirmation_required'],
        },
      }),
    })

    expect(model.actions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'cleanup',
          enabled: true,
          preview: expect.objectContaining({
            destructive: true,
            requiresConfirmation: true,
          }),
        }),
        expect.objectContaining({
          id: 'reinstall',
          enabled: true,
          preview: expect.objectContaining({
            summary: 'Reinstall requires explicit confirmation.',
          }),
        }),
      ]),
    )
  })
})

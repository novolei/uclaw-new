import { describe, expect, it } from 'vitest'
import {
  DEFAULT_STARTUP_DOCTOR_CHECKS,
  clampStartupProgress,
  deriveStartupDoctorViewModel,
  deriveStartupDoctorViewModelFromRuntimePackStatus,
  mergeRuntimePackStatusIntoStartupChecks,
  type StartupRuntimePackStatusReport,
  type StartupDoctorCheck,
} from './startup-doctor'

describe('startup doctor view model', () => {
  it('clamps progress into a stable percentage range', () => {
    expect(clampStartupProgress(-12)).toBe(0)
    expect(clampStartupProgress(36.6)).toBe(37)
    expect(clampStartupProgress(120)).toBe(100)
    expect(clampStartupProgress(Number.NaN)).toBe(0)
  })

  it('defaults to a concise checking state', () => {
    const model = deriveStartupDoctorViewModel()

    expect(model.phase).toBe('checking')
    expect(model.statusLine).toBe('Preparing uClaw')
    expect(model.progress).toBe(0)
    expect(model.detailsRecommended).toBe(false)
    expect(model.checks).toHaveLength(DEFAULT_STARTUP_DOCTOR_CHECKS.length)
  })

  it('marks ready only when every check has passed', () => {
    const checks = DEFAULT_STARTUP_DOCTOR_CHECKS.map((check) => ({
      ...check,
      status: 'passed' as const,
    }))

    const model = deriveStartupDoctorViewModel(checks)

    expect(model.phase).toBe('ready')
    expect(model.statusLine).toBe('uClaw is ready')
    expect(model.progress).toBe(100)
  })

  it('recommends details for failed or degraded startup states', () => {
    const failedChecks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'browser-runtime-pack', label: 'Runtime pack path', status: 'failed' },
    ]
    const warningChecks: StartupDoctorCheck[] = [
      { id: 'config', label: 'Local configuration', status: 'passed' },
      { id: 'network', label: 'Network availability', status: 'warning' },
    ]

    expect(deriveStartupDoctorViewModel(failedChecks)).toMatchObject({
      phase: 'failed',
      detailsRecommended: true,
    })
    expect(deriveStartupDoctorViewModel(warningChecks)).toMatchObject({
      phase: 'degraded',
      detailsRecommended: true,
    })
  })
})

function runtimeReport(
  overrides: Partial<StartupRuntimePackStatusReport> = {},
): StartupRuntimePackStatusReport {
  const base: StartupRuntimePackStatusReport = {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: [
      'browser.runtime.manifest.checked',
      'browser.runtime.filesystem.probed',
      'browser.runtime.doctor.completed',
    ],
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: false,
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

describe('startup doctor runtime-pack adapter', () => {
  it('marks browser runtime checks passed when the runtime pack is ready', () => {
    const checks = mergeRuntimePackStatusIntoStartupChecks(runtimeReport())

    expect(checks).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'network', status: 'passed' }),
        expect.objectContaining({ id: 'browser-runtime-manifest', status: 'passed' }),
        expect.objectContaining({ id: 'browser-runtime-pack', status: 'passed' }),
        expect.objectContaining({ id: 'last-runtime-status', status: 'passed' }),
      ]),
    )
  })

  it('keeps offline runtime preparation visible as a warning instead of launch failure', () => {
    const checks = mergeRuntimePackStatusIntoStartupChecks(
      runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'retry_when_online',
        doctor: {
          status: 'deferred',
          ready: false,
          issue: 'offline_download',
          remediation: 'Browser runtime preparation is waiting for network access.',
          actions: ['retry_when_online', 'defer'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: false,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'deferred',
          summary: 'Runtime preparation is deferred until network is available.',
          eventNames: ['browser.runtime.prepare.deferred'],
        },
      }),
    )

    expect(checks).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'network', status: 'warning' }),
        expect.objectContaining({ id: 'browser-runtime-pack', status: 'warning' }),
        expect.objectContaining({ id: 'last-runtime-status', status: 'warning' }),
      ]),
    )
  })

  it('derives attention state for repairable runtime-pack problems', () => {
    const model = deriveStartupDoctorViewModelFromRuntimePackStatus(
      runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'repair',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'corrupt_cache',
          remediation: 'Repair the Browser runtime pack before running Playwright providers.',
          actions: ['repair', 'reinstall'],
          manifestPackVersion: '1.48.2-uclaw.1',
          rollbackAvailable: true,
          activeTasks: 0,
        },
        operationPlan: {
          status: 'planned',
          summary: 'Repair Browser runtime pack after policy checks.',
          eventNames: ['browser.runtime.repair.planned'],
        },
      }),
    )

    expect(model.phase).toBe('degraded')
    expect(model.detailsRecommended).toBe(true)
    expect(model.checks).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'browser-runtime-pack',
          status: 'warning',
          detail: 'Repair the Browser runtime pack before running Playwright providers.',
        }),
      ]),
    )
  })

  it('marks blocked runtime-pack operation plans as failed recovery state', () => {
    const model = deriveStartupDoctorViewModelFromRuntimePackStatus(
      runtimeReport({
        ready: false,
        canRunBrowserTasks: false,
        primaryAction: 'rollback',
        doctor: {
          status: 'needs_repair',
          ready: false,
          issue: 'worker_startup_failure',
          remediation: 'Browser runtime worker failed to start.',
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
    )

    expect(model.phase).toBe('failed')
    expect(model.detailsRecommended).toBe(true)
    expect(model.checks).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'browser-runtime-pack',
          status: 'failed',
          detail: 'Rollback is blocked because no previous runtime pack exists.',
        }),
      ]),
    )
  })
})

import { describe, expect, it } from 'vitest'
import {
  DEFAULT_STARTUP_DOCTOR_CHECKS,
  clampStartupProgress,
  deriveStartupDoctorViewModel,
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

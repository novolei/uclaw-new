export type StartupDoctorCheckId =
  | 'config'
  | 'database'
  | 'bun-runtime'
  | 'permissions'
  | 'network'
  | 'browser-runtime-manifest'
  | 'browser-runtime-pack'
  | 'last-runtime-status'

export type StartupDoctorCheckStatus = 'pending' | 'running' | 'passed' | 'warning' | 'failed'

export interface StartupDoctorCheck {
  id: StartupDoctorCheckId
  label: string
  status: StartupDoctorCheckStatus
  detail?: string
}

export type StartupDoctorPhase = 'brand' | 'checking' | 'ready' | 'degraded' | 'failed'

export interface StartupDoctorViewModel {
  phase: StartupDoctorPhase
  statusLine: string
  progress: number
  checks: StartupDoctorCheck[]
  detailsRecommended: boolean
}

export const DEFAULT_STARTUP_DOCTOR_CHECKS: StartupDoctorCheck[] = [
  { id: 'config', label: 'Local configuration', status: 'running' },
  { id: 'database', label: 'Database readiness', status: 'pending' },
  { id: 'bun-runtime', label: 'Bundled Bun runtime', status: 'pending' },
  { id: 'permissions', label: 'App permissions', status: 'pending' },
  { id: 'network', label: 'Network availability', status: 'pending' },
  { id: 'browser-runtime-manifest', label: 'Browser runtime manifest', status: 'pending' },
  { id: 'browser-runtime-pack', label: 'Runtime pack path', status: 'pending' },
  { id: 'last-runtime-status', label: 'Last runtime status', status: 'pending' },
]

export function clampStartupProgress(progress: number): number {
  if (!Number.isFinite(progress)) return 0
  return Math.max(0, Math.min(100, Math.round(progress)))
}

export function deriveStartupDoctorViewModel(
  checks: StartupDoctorCheck[] = DEFAULT_STARTUP_DOCTOR_CHECKS,
): StartupDoctorViewModel {
  const total = Math.max(checks.length, 1)
  const passed = checks.filter((check) => check.status === 'passed').length
  const hasFailed = checks.some((check) => check.status === 'failed')
  const hasWarning = checks.some((check) => check.status === 'warning')
  const hasRunning = checks.some((check) => check.status === 'running')
  const progress = clampStartupProgress((passed / total) * 100)

  if (hasFailed) {
    return {
      phase: 'failed',
      statusLine: 'Startup doctor needs attention',
      progress,
      checks,
      detailsRecommended: true,
    }
  }

  if (hasWarning) {
    return {
      phase: 'degraded',
      statusLine: 'uClaw can continue while one check recovers',
      progress,
      checks,
      detailsRecommended: true,
    }
  }

  if (!hasRunning && passed === checks.length) {
    return {
      phase: 'ready',
      statusLine: 'uClaw is ready',
      progress: 100,
      checks,
      detailsRecommended: false,
    }
  }

  return {
    phase: hasRunning ? 'checking' : 'brand',
    statusLine: 'Preparing uClaw',
    progress,
    checks,
    detailsRecommended: false,
  }
}

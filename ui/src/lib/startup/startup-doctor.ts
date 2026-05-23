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

export type BrowserRuntimePackDoctorStatus =
  | 'ready'
  | 'needs_prepare'
  | 'needs_repair'
  | 'needs_update'
  | 'deferred'
  | 'degraded'

export type BrowserRuntimePackIssue =
  | 'missing_manifest'
  | 'missing_node_runtime'
  | 'missing_playwright_package'
  | 'missing_browser_binary'
  | 'corrupt_cache'
  | 'version_mismatch'
  | 'worker_startup_failure'
  | 'offline_download'
  | 'failed_real_page_probe'

export type BrowserRuntimePackAction =
  | 'prepare'
  | 'repair'
  | 'reinstall'
  | 'cleanup'
  | 'rollback'
  | 'defer'
  | 'retry_when_online'
  | 'keep_current'

export type BrowserRuntimePackPlanStatus =
  | 'ready'
  | 'planned'
  | 'requires_confirmation'
  | 'deferred'
  | 'blocked'

export interface StartupRuntimePackDoctorStatus {
  status: BrowserRuntimePackDoctorStatus
  ready: boolean
  issue?: BrowserRuntimePackIssue
  remediation: string
  actions: BrowserRuntimePackAction[]
  manifestPackVersion: string
  rollbackAvailable: boolean
  activeTasks: number
}

export interface StartupRuntimePackOperationPlan {
  status: BrowserRuntimePackPlanStatus
  summary: string
  eventNames?: string[]
}

export interface StartupRuntimePackStatusReport {
  manifestPackVersion: string
  doctor: StartupRuntimePackDoctorStatus
  primaryAction: BrowserRuntimePackAction
  operationPlan: StartupRuntimePackOperationPlan
  ready: boolean
  canRunBrowserTasks: boolean
  eventNames: string[]
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

export function mergeRuntimePackStatusIntoStartupChecks(
  report: StartupRuntimePackStatusReport | undefined,
  checks: StartupDoctorCheck[] = DEFAULT_STARTUP_DOCTOR_CHECKS,
): StartupDoctorCheck[] {
  if (!report) return checks.map((check) => ({ ...check }))

  const runtimeStatus = runtimePackCheckStatus(report)
  const lastStatus = lastRuntimeStatusCheckStatus(report)
  const manifestStatus: StartupDoctorCheckStatus =
    report.doctor.issue === 'missing_manifest' ? runtimeStatus : report.ready ? 'passed' : 'warning'
  const networkStatus: StartupDoctorCheckStatus =
    report.doctor.issue === 'offline_download' ? 'warning' : 'passed'

  return checks.map((check) => {
    if (check.id === 'network') {
      return {
        ...check,
        status: networkStatus,
        detail: report.doctor.issue === 'offline_download' ? report.doctor.remediation : undefined,
      }
    }

    if (check.id === 'browser-runtime-manifest') {
      return {
        ...check,
        status: manifestStatus,
        detail: report.ready
          ? `Runtime pack ${report.manifestPackVersion} manifest is current.`
          : report.doctor.remediation,
      }
    }

    if (check.id === 'browser-runtime-pack') {
      return {
        ...check,
        status: runtimeStatus,
        detail: runtimePackDetail(report),
      }
    }

    if (check.id === 'last-runtime-status') {
      return {
        ...check,
        status: lastStatus,
        detail: lastRuntimeStatusDetail(report),
      }
    }

    return { ...check }
  })
}

export function deriveStartupDoctorViewModelFromRuntimePackStatus(
  report: StartupRuntimePackStatusReport | undefined,
  checks: StartupDoctorCheck[] = DEFAULT_STARTUP_DOCTOR_CHECKS,
): StartupDoctorViewModel {
  return deriveStartupDoctorViewModel(mergeRuntimePackStatusIntoStartupChecks(report, checks))
}

function runtimePackCheckStatus(report: StartupRuntimePackStatusReport): StartupDoctorCheckStatus {
  if (report.ready && report.canRunBrowserTasks) return 'passed'
  if (report.operationPlan.status === 'blocked') return 'failed'
  if (report.doctor.status === 'degraded') return 'failed'
  return 'warning'
}

function lastRuntimeStatusCheckStatus(report: StartupRuntimePackStatusReport): StartupDoctorCheckStatus {
  if (report.ready && report.canRunBrowserTasks) return 'passed'
  if (report.operationPlan.status === 'blocked' || report.doctor.status === 'degraded') return 'failed'
  if (report.operationPlan.status === 'deferred') return 'warning'
  return 'warning'
}

function runtimePackDetail(report: StartupRuntimePackStatusReport): string {
  if (report.ready && report.canRunBrowserTasks) {
    return `Browser runtime pack ${report.manifestPackVersion} can run browser tasks.`
  }

  if (report.operationPlan.status === 'requires_confirmation') {
    return `${report.operationPlan.summary} Confirmation is required before ${report.primaryAction}.`
  }

  if (report.operationPlan.status === 'blocked') {
    return report.operationPlan.summary
  }

  return report.doctor.remediation
}

function lastRuntimeStatusDetail(report: StartupRuntimePackStatusReport): string {
  const reportEvents = report.eventNames
  const planEvents = report.operationPlan.eventNames ?? []
  const latestEvent =
    reportEvents.length > 0
      ? reportEvents[reportEvents.length - 1]
      : planEvents.length > 0
        ? planEvents[planEvents.length - 1]
        : undefined
  const suffix = latestEvent ? ` Latest event: ${latestEvent}.` : ''

  if (report.ready && report.canRunBrowserTasks) {
    return `Last runtime status is ready.${suffix}`
  }

  return `${report.doctor.remediation}${suffix}`
}

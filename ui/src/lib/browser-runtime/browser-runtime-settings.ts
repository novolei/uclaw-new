import type {
  BrowserRuntimePackAction,
  StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor'

export type BrowserRuntimeSettingsStatusKind =
  | 'unknown'
  | 'ready'
  | 'attention'
  | 'blocked'
  | 'deferred'

export type BrowserRuntimeSettingsUpdateState =
  | 'current'
  | 'available'
  | 'security'
  | 'deferred'
  | 'unknown'

export interface BrowserRuntimeSettingsInput {
  report?: StartupRuntimePackStatusReport
  lastCheckedAtMs?: number
  artifactSizeBytes?: number
  runtimePackPath?: string
  releaseChannel?: string
  updateState?: BrowserRuntimeSettingsUpdateState
  developerFallbackEnabled?: boolean
  autoPrepareEnabled?: boolean
}

export interface BrowserRuntimeSettingsAction {
  id: BrowserRuntimePackAction | 'run_doctor'
  label: string
  enabled: boolean
}

export interface BrowserRuntimeSettingsViewModel {
  statusKind: BrowserRuntimeSettingsStatusKind
  statusLabel: string
  statusDetail: string
  lastCheckedLabel: string
  versionLabel: string
  artifactSizeLabel: string
  runtimePackPathLabel: string
  releaseChannelLabel: string
  updateStateLabel: string
  rollbackLabel: string
  developerFallbackLabel: string
  autoPrepareLabel: string
  actions: BrowserRuntimeSettingsAction[]
}

const ACTION_LABELS: Record<BrowserRuntimeSettingsAction['id'], string> = {
  prepare: '准备',
  repair: '修复',
  reinstall: '重装',
  cleanup: '清理',
  rollback: '回滚',
  defer: '稍后',
  retry_when_online: '联网后重试',
  keep_current: '保持当前',
  run_doctor: '运行诊断',
}

export function deriveBrowserRuntimeSettingsViewModel(
  input: BrowserRuntimeSettingsInput = {},
): BrowserRuntimeSettingsViewModel {
  const report = input.report
  const statusKind = deriveStatusKind(report)
  const actions = deriveActions(report)

  return {
    statusKind,
    statusLabel: statusLabel(statusKind),
    statusDetail: statusDetail(report),
    lastCheckedLabel: formatLastChecked(input.lastCheckedAtMs),
    versionLabel: report?.manifestPackVersion ?? '未检查',
    artifactSizeLabel: formatArtifactSize(input.artifactSizeBytes),
    runtimePackPathLabel: input.runtimePackPath ?? '等待运行时状态',
    releaseChannelLabel: input.releaseChannel ?? 'stable',
    updateStateLabel: updateStateLabel(input.updateState ?? 'unknown'),
    rollbackLabel: report?.doctor.rollbackAvailable ? '可用' : '不可用',
    developerFallbackLabel: input.developerFallbackEnabled ? '已启用' : '未启用',
    autoPrepareLabel: input.autoPrepareEnabled === false ? '已关闭' : '已开启',
    actions,
  }
}

function deriveStatusKind(
  report: StartupRuntimePackStatusReport | undefined,
): BrowserRuntimeSettingsStatusKind {
  if (!report) return 'unknown'
  if (report.ready && report.canRunBrowserTasks) return 'ready'
  if (report.operationPlan.status === 'blocked' || report.doctor.status === 'degraded') {
    return 'blocked'
  }
  if (report.operationPlan.status === 'deferred' || report.doctor.status === 'deferred') {
    return 'deferred'
  }
  return 'attention'
}

function statusLabel(kind: BrowserRuntimeSettingsStatusKind): string {
  switch (kind) {
    case 'ready':
      return '可用'
    case 'attention':
      return '需要处理'
    case 'blocked':
      return '受阻'
    case 'deferred':
      return '已推迟'
    case 'unknown':
    default:
      return '未检查'
  }
}

function statusDetail(report: StartupRuntimePackStatusReport | undefined): string {
  if (!report) return '等待 Startup Doctor 提供浏览器运行时状态。'
  if (report.ready && report.canRunBrowserTasks) return report.operationPlan.summary
  return report.operationPlan.summary || report.doctor.remediation
}

function deriveActions(
  report: StartupRuntimePackStatusReport | undefined,
): BrowserRuntimeSettingsAction[] {
  const actionIds = report?.doctor.actions.length
    ? report.doctor.actions
    : (['prepare', 'repair', 'reinstall', 'cleanup', 'rollback'] as BrowserRuntimePackAction[])

  return [
    ...actionIds.map((id) => ({
      id,
      label: ACTION_LABELS[id],
      enabled: false,
    })),
    {
      id: 'run_doctor' as const,
      label: ACTION_LABELS.run_doctor,
      enabled: false,
    },
  ]
}

function formatLastChecked(lastCheckedAtMs: number | undefined): string {
  if (!lastCheckedAtMs) return '未检查'
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(lastCheckedAtMs))
}

function formatArtifactSize(bytes: number | undefined): string {
  if (!bytes || bytes <= 0) return '未知'
  const mib = bytes / (1024 * 1024)
  if (mib >= 1024) return `${(mib / 1024).toFixed(1)} GiB`
  return `${mib.toFixed(0)} MiB`
}

function updateStateLabel(state: BrowserRuntimeSettingsUpdateState): string {
  switch (state) {
    case 'current':
      return '当前版本'
    case 'available':
      return '有更新'
    case 'security':
      return '安全更新'
    case 'deferred':
      return '已推迟'
    case 'unknown':
    default:
      return '未知'
  }
}

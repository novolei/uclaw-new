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

export type BrowserRuntimeSettingsActionId =
  | BrowserRuntimePackAction
  | 'run_doctor'
  | 'disable_auto_prepare'
  | 'enable_auto_prepare'

export interface BrowserRuntimeSettingsAction {
  id: BrowserRuntimeSettingsActionId
  label: string
  enabled: boolean
  preview: BrowserRuntimeSettingsActionPreview
}

export interface BrowserRuntimeSettingsActionPreview {
  title: string
  summary: string
  eventNames: string[]
  requiresConfirmation: boolean
  destructive: boolean
}

export interface BrowserRuntimeSettingsViewModel {
  statusKind: BrowserRuntimeSettingsStatusKind
  statusLabel: string
  statusDetail: string
  lastCheckedLabel: string
  versionLabel: string
  artifactSizeLabel: string
  runtimeRootLabel: string
  runtimePackPathLabel: string
  releaseChannelLabel: string
  updateStateLabel: string
  rollbackLabel: string
  developerFallbackLabel: string
  autoPrepareLabel: string
  actions: BrowserRuntimeSettingsAction[]
}

const ACTION_LABELS: Record<BrowserRuntimeSettingsActionId, string> = {
  prepare: '准备',
  repair: '修复',
  reinstall: '重装',
  cleanup: '清理',
  rollback: '回滚',
  defer: '稍后',
  retry_when_online: '联网后重试',
  keep_current: '保持当前',
  disable_auto_prepare: '关闭自动准备',
  enable_auto_prepare: '开启自动准备',
  run_doctor: '运行诊断',
}

export function deriveBrowserRuntimeSettingsViewModel(
  input: BrowserRuntimeSettingsInput = {},
): BrowserRuntimeSettingsViewModel {
  const report = input.report
  const statusKind = deriveStatusKind(report)
  const actions = deriveActions(report, input.autoPrepareEnabled)

  return {
    statusKind,
    statusLabel: statusLabel(statusKind),
    statusDetail: statusDetail(report),
    lastCheckedLabel: formatLastChecked(input.lastCheckedAtMs),
    versionLabel: report?.manifestPackVersion ?? '未检查',
    artifactSizeLabel: formatArtifactSize(input.artifactSizeBytes),
    runtimeRootLabel: report?.runtimeRoot ?? '等待运行时状态',
    runtimePackPathLabel: report?.currentPackDir ?? input.runtimePackPath ?? '等待运行时状态',
    releaseChannelLabel: input.releaseChannel ?? 'stable',
    updateStateLabel: updateStateLabel(input.updateState ?? 'unknown'),
    rollbackLabel: report?.doctor.rollbackAvailable ? '可用' : '不可用',
    developerFallbackLabel: input.developerFallbackEnabled ? '已启用' : '未启用',
    autoPrepareLabel: input.autoPrepareEnabled === undefined
      ? '等待运行时状态'
      : input.autoPrepareEnabled ? '已开启' : '已关闭',
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
  autoPrepareEnabled: boolean | undefined,
): BrowserRuntimeSettingsAction[] {
  const actionIds = report?.doctor.actions.length
    ? report.doctor.actions
    : (['prepare', 'repair', 'reinstall', 'cleanup', 'rollback'] as BrowserRuntimePackAction[])
  const autoPrepareActionId: BrowserRuntimeSettingsActionId = autoPrepareEnabled === false
    ? 'enable_auto_prepare'
    : 'disable_auto_prepare'

  return [
    ...actionIds.map((id) => ({
      id,
      label: ACTION_LABELS[id],
      enabled: Boolean(report),
      preview: actionPreview(id, report),
    })),
    {
      id: autoPrepareActionId,
      label: ACTION_LABELS[autoPrepareActionId],
      enabled: autoPrepareEnabled !== undefined,
      preview: actionPreview(autoPrepareActionId, report),
    },
    {
      id: 'run_doctor' as const,
      label: ACTION_LABELS.run_doctor,
      enabled: Boolean(report),
      preview: actionPreview('run_doctor', report),
    },
  ]
}

function actionPreview(
  id: BrowserRuntimeSettingsActionId,
  report: StartupRuntimePackStatusReport | undefined,
): BrowserRuntimeSettingsActionPreview {
  const isPrimary = report?.primaryAction === id
  const eventNames = actionEventNames(id, report)
  const fallbackSummary = report
    ? report.doctor.remediation
    : '等待 Startup Doctor 提供浏览器运行时状态。'
  const summary = isPrimary
    ? report?.operationPlan.summary ?? fallbackSummary
    : actionSummary(id, fallbackSummary)

  return {
    title: ACTION_LABELS[id],
    summary,
    eventNames: uniqueEventNames(eventNames),
    requiresConfirmation: id === 'reinstall'
      || id === 'cleanup'
      || id === 'rollback'
      || report?.operationPlan.status === 'requires_confirmation',
    destructive: id === 'cleanup' || id === 'rollback' || id === 'reinstall',
  }
}

function actionSummary(
  id: BrowserRuntimeSettingsActionId,
  fallbackSummary: string,
): string {
  switch (id) {
    case 'prepare':
      return '准备 pinned Browser runtime pack，等待后端执行边界接入。'
    case 'repair':
      return '修复当前 Browser runtime pack，等待后端执行边界接入。'
    case 'reinstall':
      return '重装 Browser runtime pack，需要明确确认并等待后端执行边界接入。'
    case 'cleanup':
      return '清理旧 Browser runtime artifacts，需要明确确认并等待后端执行边界接入。'
    case 'rollback':
      return '回滚到上一个可用 Browser runtime pack，需要明确确认并等待后端执行边界接入。'
    case 'defer':
      return '推迟 runtime preparation，后续 task-time prompt 会继续这条路径。'
    case 'retry_when_online':
      return '网络恢复后重试 Browser runtime preparation。'
    case 'keep_current':
      return '保持当前 Browser runtime pack，不执行准备或修复。'
    case 'disable_auto_prepare':
      return '关闭启动/后台自动准备；浏览器任务仍可在使用时请求准备运行时。'
    case 'enable_auto_prepare':
      return '恢复启动/后台自动准备；不会立即下载或修复运行时。'
    case 'run_doctor':
      return '刷新 Startup Doctor / Browser Runtime 状态，只读取本地运行时状态。'
    default:
      return fallbackSummary
  }
}

function actionEventNames(
  id: BrowserRuntimeSettingsActionId,
  report: StartupRuntimePackStatusReport | undefined,
): string[] {
  if (id === 'disable_auto_prepare') {
    return ['browser.runtime.auto_prepare.disable.requested']
  }
  if (id === 'enable_auto_prepare') {
    return ['browser.runtime.auto_prepare.enable.requested']
  }

  return [
    ...(report?.operationPlan.eventNames ?? []),
    ...(report?.eventNames ?? []),
  ]
}

function uniqueEventNames(eventNames: string[]): string[] {
  return Array.from(new Set(eventNames.filter(Boolean)))
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

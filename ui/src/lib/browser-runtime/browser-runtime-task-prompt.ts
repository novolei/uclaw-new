import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'

export type BrowserRuntimeTaskTimePromptStatus =
  | 'ready'
  | 'prepare_required'
  | 'confirmation_required'
  | 'deferred'
  | 'blocked'

export type BrowserRuntimeTaskTimePromptActionId =
  | 'prepare_now'
  | 'defer'
  | 'continue_without_browser'

export type BrowserRuntimePreparationDecision = 'ready' | 'defer'

export interface BrowserTaskRuntimeDecisionPayload {
  runtime_preparation_decision: BrowserRuntimePreparationDecision
}

export interface BrowserRuntimeTaskTimePromptInput {
  report?: StartupRuntimePackStatusReport
  browserRequired: boolean
  noBrowserFallbackAvailable: boolean
  taskLabel?: string
}

export interface BrowserRuntimeTaskTimePromptAction {
  id: BrowserRuntimeTaskTimePromptActionId
  label: string
  enabled: boolean
  primary: boolean
  summary: string
  eventNames: string[]
  checkpointStatus?: 'paused_waiting_for_browser_runtime'
  browserTaskRequestPatch?: BrowserTaskRuntimeDecisionPayload
}

export interface BrowserRuntimeTaskTimePromptViewModel {
  shouldShowPrompt: boolean
  status: BrowserRuntimeTaskTimePromptStatus
  title: string
  summary: string
  actions: BrowserRuntimeTaskTimePromptAction[]
}

export function deriveBrowserRuntimeTaskTimePrompt(
  input: BrowserRuntimeTaskTimePromptInput,
): BrowserRuntimeTaskTimePromptViewModel {
  const report = input.report

  if (report?.ready && report.canRunBrowserTasks) {
    return {
      shouldShowPrompt: false,
      status: 'ready',
      title: '浏览器运行时可用',
      summary: `${taskName(input)} 可以继续使用 Browser runtime。`,
      actions: [],
    }
  }

  const status = promptStatus(report)
  const canPrepare = Boolean(report)
    && status !== 'blocked'
    && report?.operationPlan.status !== 'deferred'
  const checkpointOnDefer = input.browserRequired && !input.noBrowserFallbackAvailable

  return {
    shouldShowPrompt: true,
    status,
    title: promptTitle(status),
    summary: promptSummary(input, status),
    actions: [
      {
        id: 'prepare_now',
        label: '现在准备',
        enabled: canPrepare,
        primary: canPrepare,
        summary: prepareSummary(report),
        eventNames: prepareEventNames(report),
      },
      {
        id: 'defer',
        label: '稍后处理',
        enabled: true,
        primary: !canPrepare && !input.noBrowserFallbackAvailable,
        summary: checkpointOnDefer
          ? '暂停当前任务，等待 Browser runtime 准备完成后恢复。'
          : '推迟 Browser runtime 准备；当前任务可继续走无浏览器路径。',
        eventNames: [
          checkpointOnDefer
            ? 'browser.runtime.task_time.defer.checkpointed'
            : 'browser.runtime.task_time.defer.recorded',
        ],
        checkpointStatus: checkpointOnDefer
          ? 'paused_waiting_for_browser_runtime'
          : undefined,
        browserTaskRequestPatch: checkpointOnDefer
          ? { runtime_preparation_decision: 'defer' }
          : undefined,
      },
      {
        id: 'continue_without_browser',
        label: '不用浏览器继续',
        enabled: input.noBrowserFallbackAvailable,
        primary: !canPrepare && input.noBrowserFallbackAvailable,
        summary: input.noBrowserFallbackAvailable
          ? '使用无浏览器能力继续当前任务。'
          : '当前任务没有可用的无浏览器替代路径。',
        eventNames: ['browser.runtime.task_time.no_browser.continued'],
      },
    ],
  }
}

export function browserTaskRuntimeDecisionPayloadForAction(
  action: BrowserRuntimeTaskTimePromptAction,
): BrowserTaskRuntimeDecisionPayload | undefined {
  return action.browserTaskRequestPatch
}

function promptStatus(
  report: StartupRuntimePackStatusReport | undefined,
): BrowserRuntimeTaskTimePromptStatus {
  if (!report) return 'prepare_required'
  if (report.operationPlan.status === 'blocked' || report.doctor.status === 'degraded') {
    return 'blocked'
  }
  if (report.operationPlan.status === 'requires_confirmation') {
    return 'confirmation_required'
  }
  if (report.operationPlan.status === 'deferred' || report.doctor.status === 'deferred') {
    return 'deferred'
  }
  return 'prepare_required'
}

function promptTitle(status: BrowserRuntimeTaskTimePromptStatus): string {
  switch (status) {
    case 'confirmation_required':
      return '准备浏览器运行时'
    case 'deferred':
      return '浏览器运行时已推迟'
    case 'blocked':
      return '浏览器运行时受阻'
    case 'prepare_required':
    default:
      return '需要准备浏览器运行时'
  }
}

function promptSummary(
  input: BrowserRuntimeTaskTimePromptInput,
  status: BrowserRuntimeTaskTimePromptStatus,
): string {
  const report = input.report
  const task = taskName(input)

  if (!report) {
    return `${task} 需要 Browser runtime，但当前还没有可用状态报告。`
  }
  if (status === 'blocked') {
    return `${task} 需要 Browser runtime，但当前准备受阻：${report.operationPlan.summary}`
  }
  if (status === 'deferred') {
    return `${task} 需要 Browser runtime；准备已被推迟，可稍后恢复。`
  }
  if (status === 'confirmation_required') {
    return `${task} 需要 Browser runtime；继续前需要确认准备操作。`
  }
  return `${task} 需要 Browser runtime；可以现在准备、稍后处理，或在可行时使用无浏览器路径。`
}

function prepareSummary(report: StartupRuntimePackStatusReport | undefined): string {
  if (!report) return '等待 Browser runtime 状态报告后才能准备。'
  if (report.operationPlan.status === 'requires_confirmation') {
    return report.operationPlan.summary
  }
  if (report.operationPlan.status === 'deferred') {
    return '准备已推迟；后续恢复需要新的任务时间确认。'
  }
  if (report.operationPlan.status === 'blocked') {
    return report.operationPlan.summary
  }
  return report.operationPlan.summary || report.doctor.remediation
}

function prepareEventNames(report: StartupRuntimePackStatusReport | undefined): string[] {
  return uniqueEventNames([
    'browser.runtime.task_time.prepare.requested',
    ...(report?.operationPlan.eventNames ?? []),
    ...(report?.eventNames ?? []),
  ])
}

function taskName(input: BrowserRuntimeTaskTimePromptInput): string {
  return input.taskLabel?.trim() || '当前任务'
}

function uniqueEventNames(eventNames: string[]): string[] {
  return Array.from(new Set(eventNames.filter(Boolean)))
}

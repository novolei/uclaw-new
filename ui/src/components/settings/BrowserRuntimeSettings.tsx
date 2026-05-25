import * as React from 'react'
import {
  Activity,
  Archive,
  Bug,
  Download,
  HardDrive,
  History,
  KeyRound,
  LogOut,
  Power,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  deriveBrowserRuntimeControlCenterViewModel,
  priorityWithProviderFirst,
  type BrowserRuntimeProviderRowViewModel,
} from '@/lib/browser-runtime/browser-runtime-control-center'
import {
  deriveBrowserRuntimeSettingsViewModel,
  type BrowserRuntimeSettingsInput,
  type BrowserRuntimeSettingsAction,
} from '@/lib/browser-runtime/browser-runtime-settings'
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimePackAction,
  BrowserRuntimePackExecutionReport,
  BrowserRuntimeProviderId,
} from '@/lib/startup/startup-doctor'
import {
  dryRunBrowserRuntimeAction,
  getBrowserRuntimeControlCenter,
  getBrowserRuntimeStatus,
  listBrowserIdentities,
  revokeBrowserIdentity,
  setBrowserRuntimeProviderEnabled,
  setBrowserRuntimeProviderPriority,
  type BrowserIdentityActiveTaskSummary,
  type BrowserIdentityProfileSummary,
  type BrowserIdentityStatusReport,
} from '@/lib/tauri-bridge'
import { SettingsCard, SettingsRow, SettingsSection } from './primitives'

interface BrowserRuntimeSettingsProps {
  status?: BrowserRuntimeSettingsInput
}

const ACTION_ICONS: Record<BrowserRuntimeSettingsAction['id'], React.ReactNode> = {
  prepare: <Download />,
  repair: <RefreshCw />,
  reinstall: <Archive />,
  cleanup: <Trash2 />,
  rollback: <RotateCcw />,
  defer: <History />,
  retry_when_online: <RefreshCw />,
  keep_current: <ShieldCheck />,
  disable_auto_prepare: <Power />,
  enable_auto_prepare: <Power />,
  run_doctor: <Bug />,
}

const DRY_RUN_ACTIONS = new Set<BrowserRuntimeSettingsAction['id']>([
  'prepare',
  'repair',
  'reinstall',
  'cleanup',
  'rollback',
  'keep_current',
])

export function BrowserRuntimeSettings({
  status,
}: BrowserRuntimeSettingsProps): React.ReactElement {
  const [liveStatus, setLiveStatus] = React.useState<BrowserRuntimeSettingsInput | undefined>()
  const [dryRunReports, setDryRunReports] = React.useState<
    Partial<Record<BrowserRuntimeSettingsAction['id'], BrowserRuntimePackExecutionReport>>
  >({})
  const [dryRunPendingActionId, setDryRunPendingActionId] =
    React.useState<BrowserRuntimeSettingsAction['id'] | null>(null)
  const [dryRunErrors, setDryRunErrors] = React.useState<
    Partial<Record<BrowserRuntimeSettingsAction['id'], string>>
  >({})
  const [identityStatus, setIdentityStatus] = React.useState<BrowserIdentityStatusReport | undefined>()
  const [revokingProfileId, setRevokingProfileId] = React.useState<string | null>(null)
  const [controlCenter, setControlCenter] = React.useState<BrowserRuntimeControlCenterReport | undefined>(
    status?.report?.controlCenter,
  )
  const [controlCenterError, setControlCenterError] = React.useState<string | undefined>()
  const [controlCenterPendingAction, setControlCenterPendingAction] = React.useState<string | null>(null)
  const refreshGenerationRef = React.useRef(0)
  const dryRunGenerationRef = React.useRef(0)
  const identityGenerationRef = React.useRef(0)
  const controlCenterGenerationRef = React.useRef(0)
  const mountedRef = React.useRef(false)

  React.useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      refreshGenerationRef.current += 1
      dryRunGenerationRef.current += 1
      identityGenerationRef.current += 1
      controlCenterGenerationRef.current += 1
    }
  }, [])

  const refreshLiveStatus = React.useCallback(async () => {
    if (status) return

    const generation = refreshGenerationRef.current + 1
    refreshGenerationRef.current = generation

    try {
      const report = await getBrowserRuntimeStatus()
      if (mountedRef.current && refreshGenerationRef.current === generation) {
        setLiveStatus({
          report,
          lastCheckedAtMs: Date.now(),
        })
        if (report.controlCenter) {
          setControlCenter(report.controlCenter)
          setControlCenterError(undefined)
        }
      }
    } catch {
      // Keep the last displayed status when a manual refresh fails.
    }
  }, [status])

  const refreshControlCenter = React.useCallback(async () => {
    if (status?.report?.controlCenter) {
      setControlCenter(status.report.controlCenter)
      setControlCenterError(undefined)
      return
    }

    const generation = controlCenterGenerationRef.current + 1
    controlCenterGenerationRef.current = generation

    try {
      const report = await getBrowserRuntimeControlCenter()
      if (mountedRef.current && controlCenterGenerationRef.current === generation) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current && controlCenterGenerationRef.current === generation) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    }
  }, [status])

  const enableProvider = React.useCallback(async (providerId: BrowserRuntimeProviderId) => {
    if (status) return

    setControlCenterPendingAction(`enable:${providerId}`)
    try {
      const report = await setBrowserRuntimeProviderEnabled(providerId, true)
      if (mountedRef.current) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [status])

  const setProviderFirst = React.useCallback(async (
    providerId: BrowserRuntimeProviderId,
    currentPriority: BrowserRuntimeProviderId[],
  ) => {
    if (status) return

    setControlCenterPendingAction(`first:${providerId}`)
    try {
      const report = await setBrowserRuntimeProviderPriority(
        priorityWithProviderFirst(currentPriority, providerId),
      )
      if (mountedRef.current) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [status])

  const dryRunAction = React.useCallback(async (actionId: BrowserRuntimeSettingsAction['id']) => {
    if (status || !isDryRunAction(actionId)) return

    const generation = dryRunGenerationRef.current + 1
    dryRunGenerationRef.current = generation
    setDryRunPendingActionId(actionId)
    setDryRunErrors((current) => {
      const next = { ...current }
      delete next[actionId]
      return next
    })
    setDryRunReports((current) => {
      const next = { ...current }
      delete next[actionId]
      return next
    })

    try {
      const report = await dryRunBrowserRuntimeAction(actionId)
      if (mountedRef.current && dryRunGenerationRef.current === generation) {
        setDryRunReports((current) => ({
          ...current,
          [actionId]: report,
        }))
      }
    } catch (error) {
      if (mountedRef.current && dryRunGenerationRef.current === generation) {
        setDryRunErrors((current) => ({
          ...current,
          [actionId]: error instanceof Error ? error.message : String(error),
        }))
        setDryRunReports((current) => {
          const next = { ...current }
          delete next[actionId]
          return next
        })
      }
    } finally {
      if (mountedRef.current && dryRunGenerationRef.current === generation) {
        setDryRunPendingActionId(null)
      }
    }
  }, [status])

  const refreshIdentityStatus = React.useCallback(async () => {
    const generation = identityGenerationRef.current + 1
    identityGenerationRef.current = generation

    try {
      const report = await listBrowserIdentities()
      if (mountedRef.current && identityGenerationRef.current === generation) {
        setIdentityStatus(report)
      }
    } catch {
      // Keep the last displayed identity status when a refresh fails.
    }
  }, [])

  const revokeIdentity = React.useCallback(async (profileId: string) => {
    if (revokingProfileId) return

    setRevokingProfileId(profileId)
    try {
      await revokeBrowserIdentity(profileId)
      await refreshIdentityStatus()
    } catch {
      // Keep the current profile list if revocation fails.
    } finally {
      if (mountedRef.current) {
        setRevokingProfileId(null)
      }
    }
  }, [refreshIdentityStatus, revokingProfileId])

  React.useEffect(() => {
    if (status) {
      refreshGenerationRef.current += 1
      setLiveStatus(undefined)
      setControlCenter(status.report?.controlCenter)
      return
    }

    void refreshLiveStatus()
  }, [refreshLiveStatus, status])

  React.useEffect(() => {
    void refreshControlCenter()
  }, [refreshControlCenter])

  React.useEffect(() => {
    void refreshIdentityStatus()
  }, [refreshIdentityStatus])

  React.useEffect(() => {
    setDryRunReports({})
    setDryRunErrors({})
    setDryRunPendingActionId(null)
  }, [liveStatus, status])

  const model = deriveBrowserRuntimeSettingsViewModel(status ?? liveStatus)
  const activeControlCenter = controlCenter ?? status?.report?.controlCenter ?? liveStatus?.report?.controlCenter
  const controlModel = deriveBrowserRuntimeControlCenterViewModel(activeControlCenter)
  const [selectedActionId, setSelectedActionId] =
    React.useState<BrowserRuntimeSettingsAction['id'] | null>(null)
  const selectedAction = selectedActionId
    ? model.actions.find((action) => action.id === selectedActionId)
    : undefined
  const selectedDryRunReport = selectedAction ? dryRunReports[selectedAction.id] : undefined
  const selectedDryRunError = selectedAction ? dryRunErrors[selectedAction.id] : undefined
  const selectedDryRunPending = selectedAction?.id === dryRunPendingActionId

  return (
    <div className="space-y-8">
      <SettingsSection
        title="Browser Runtime Control Center"
        description="CLI first · MCP second · Local Chromium fallback"
      >
        <SettingsCard>
          <SettingsRow
            label="Desired route"
            icon={<Activity size={16} />}
            description={controlModel.routeSummary.desiredLabel}
          >
            <Badge variant="outline">{controlModel.routeSummary.primaryActionLabel}</Badge>
          </SettingsRow>
          <SettingsRow
            label="Active route"
            description={controlModel.routeSummary.reasonLabel}
          >
            <Badge variant={controlModel.routeSummary.activeLabel === 'Local Chromium' ? 'secondary' : 'default'}>
              {controlModel.routeSummary.activeLabel}
            </Badge>
          </SettingsRow>
          <SettingsRow
            label="Control state"
            description={controlCenterError ?? '读取 Rust Browser Runtime Control Center 状态。'}
          >
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={Boolean(status)}
              onClick={() => {
                void refreshControlCenter()
              }}
            >
              <RefreshCw />
              刷新
            </Button>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="Provider Priority">
        <SettingsCard divided={false}>
          <div className="divide-y divide-border">
            {controlModel.providerRows.length > 0 ? (
              controlModel.providerRows.map((row) => (
                <ProviderPriorityRow
                  key={row.lane.providerId}
                  row={row}
                  priority={activeControlCenter?.desiredProviderPriority ?? []}
                  pendingAction={controlCenterPendingAction}
                  disabled={Boolean(status)}
                  onEnable={enableProvider}
                  onSetFirst={setProviderFirst}
                />
              ))
            ) : (
              <div className="p-4 text-sm text-muted-foreground">
                等待 Rust Browser Runtime Control Center 报告。
              </div>
            )}
          </div>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="运行时 Supervisor" description="Rust Browser Runtime Supervisor">
        <SettingsCard>
          <SettingsRow
            label="Supervisor"
            icon={<Activity size={16} />}
            description={model.supervisorDetailLabel}
          >
            <Badge variant={badgeVariant(model.supervisorStatusKind)}>
              {model.supervisorStateLabel}
            </Badge>
          </SettingsRow>
          <SettingsRow label="Provider" description={model.supervisorProviderLabel} />
          <SettingsRow label="Doctor" description={model.supervisorDoctorLabel} />
          <SettingsRow label="活跃上下文" description={model.supervisorActiveContextsLabel} />
          <SettingsRow label="Local Chromium" description={model.localProviderLabel} />
          <SettingsRow label="Playwright CLI" description={model.playwrightCliProviderLabel} />
          <SettingsRow label="Playwright MCP" description={model.playwrightMcpProviderLabel} />
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="Playwright runtime pack" description="uClaw-managed runtime pack">
        <SettingsCard>
          <SettingsRow label="状态" icon={<Activity size={16} />} description={model.statusDetail}>
            <Badge variant={badgeVariant(model.statusKind)}>{model.statusLabel}</Badge>
          </SettingsRow>
          <SettingsRow label="最后检查" description={model.lastCheckedLabel} />
          <SettingsRow label="版本" description={model.releaseChannelLabel}>
            <span className="text-sm font-medium">{model.versionLabel}</span>
          </SettingsRow>
          <SettingsRow label="更新状态" description={model.updateStateLabel} />
          <SettingsRow label="体积" description={model.artifactSizeLabel}>
            <HardDrive size={16} className="text-muted-foreground" />
          </SettingsRow>
          <SettingsRow label="运行时根目录" description={model.runtimeRootLabel} />
          <SettingsRow label="当前 pack" description={model.runtimePackPathLabel} />
          <SettingsRow label="回滚" description={model.rollbackLabel} />
          <SettingsRow label="开发者回退" description={model.developerFallbackLabel} />
          <SettingsRow label="自动准备" description={model.autoPrepareLabel} />
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="浏览器身份" description="uClaw-managed browser identity">
        <SettingsCard>
          <SettingsRow
            label="状态"
            icon={<KeyRound size={16} />}
            description={identityStatusDetail(identityStatus)}
          >
            <Badge variant={identityBadgeVariant(identityStatus)}>
              {identityStatusLabel(identityStatus)}
            </Badge>
          </SettingsRow>
          <SettingsRow label="授权身份" description={identityCountLabel(identityStatus)} />
          <SettingsRow label="上次使用" description={latestIdentityLastUsedLabel(identityStatus)} />
          <SettingsRow label="活跃任务" description={identityActiveTaskLabel(identityStatus)} />
        </SettingsCard>

        {identityStatus?.profiles.length ? (
          <SettingsCard divided={false}>
            <div className="divide-y divide-border">
              {identityStatus.profiles.map((profile) => (
                <div
                  key={profile.id}
                  className="flex items-center justify-between gap-4 p-4"
                >
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm font-medium">{profile.label}</span>
                      <Badge variant={profile.revoked ? 'secondary' : 'outline'}>
                        {identityProfileStatusLabel(profile)}
                      </Badge>
                    </div>
                    <div className="mt-1 truncate text-xs text-muted-foreground">
                      {profile.originPattern} · {identityProviderLabel(profile.provider)} · {identityScopeLabel(profile.scope)}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      上次使用 {formatIdentityTimestamp(profile.lastUsedAtMs)}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={profile.revoked || revokingProfileId === profile.id}
                    aria-label={profile.revoked ? `已撤销 ${profile.label}` : `撤销 ${profile.label}`}
                    onClick={() => {
                      void revokeIdentity(profile.id)
                    }}
                  >
                    <LogOut />
                    {profile.revoked ? '已撤销' : '撤销'}
                  </Button>
                </div>
              ))}
            </div>
          </SettingsCard>
        ) : null}

        {identityStatus?.activeTasks.length ? (
          <SettingsCard divided={false}>
            <div className="divide-y divide-border">
              {identityStatus.activeTasks.map((task) => (
                <div
                  key={task.runId}
                  className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto]"
                >
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-medium">{task.task}</span>
                      <Badge variant={task.drainDeadlineMs ? 'secondary' : 'outline'}>
                        {identityTaskStatusLabel(task.status)}
                      </Badge>
                    </div>
                    <div className="mt-1 truncate text-xs text-muted-foreground">
                      {task.sessionId} · {task.runId}
                    </div>
                  </div>
                  <div className="text-left text-xs text-muted-foreground md:text-right">
                    <div>更新 {formatIdentityTimestamp(task.updatedAtMs)}</div>
                    {task.drainDeadlineMs ? (
                      <div>撤销 drain 至 {formatIdentityTimestamp(task.drainDeadlineMs)}</div>
                    ) : null}
                  </div>
                </div>
              ))}
            </div>
          </SettingsCard>
        ) : null}
      </SettingsSection>

      <SettingsSection title="操作">
        <SettingsCard divided={false}>
          <div className="grid grid-cols-2 gap-2 p-4 sm:grid-cols-3">
            {model.actions.map((action) => (
              <Button
                key={action.id}
                type="button"
                variant="outline"
                size="sm"
                disabled={!action.enabled || dryRunPendingActionId === action.id}
                aria-label={actionButtonLabel(action)}
                onClick={() => {
                  setSelectedActionId(action.id)
                  if (action.id === 'run_doctor' && !status) {
                    void refreshLiveStatus()
                  } else if (DRY_RUN_ACTIONS.has(action.id) && !status) {
                    void dryRunAction(action.id)
                  }
                }}
              >
                {ACTION_ICONS[action.id]}
                {dryRunPendingActionId === action.id ? '读取中' : actionButtonLabel(action)}
              </Button>
            ))}
          </div>
        </SettingsCard>
      </SettingsSection>

      {selectedAction && (
        <SettingsSection title="操作预览">
          <SettingsCard>
            <SettingsRow
              label={selectedAction.preview.title}
              description={
                selectedDryRunPending
                  ? '正在读取后端 dry-run 计划。'
                  : selectedDryRunError ?? selectedDryRunReport?.summary ?? selectedAction.preview.summary
              }
            >
              <Badge
                variant={
                  selectedDryRunError
                    ? 'destructive'
                    : selectedDryRunReport?.destructive || selectedAction.preview.destructive
                    ? 'destructive'
                    : 'outline'
                }
              >
                {selectedDryRunError
                  ? '失败'
                  : selectedDryRunPending
                    ? '读取中'
                    : selectedDryRunReport
                      ? '后端 Dry run'
                      : selectedAction.preview.requiresConfirmation ? '本地预估 · 需确认' : '本地预估'}
              </Badge>
            </SettingsRow>
            <SettingsRow
              label="事件"
              description={
                selectedDryRunReport
                  ? selectedDryRunReport.eventNames.join(' · ')
                  : selectedDryRunPending
                    ? '等待后端 dry-run 事件'
                    : selectedAction.preview.eventNames.length > 0
                      ? selectedAction.preview.eventNames.join(' · ')
                      : '点击预览按钮后显示后端 dry-run 事件'
              }
            >
              <span className="text-xs text-muted-foreground">无副作用</span>
            </SettingsRow>
            {selectedDryRunReport ? (
              <SettingsRow
                label="Dry-run artifact"
                description={selectedDryRunReport.artifactId}
              >
                <span className="text-xs text-muted-foreground">
                  {selectedDryRunReport.stepReports.length} steps
                </span>
              </SettingsRow>
            ) : null}
          </SettingsCard>
        </SettingsSection>
      )}
    </div>
  )
}

interface ProviderPriorityRowProps {
  row: BrowserRuntimeProviderRowViewModel
  priority: BrowserRuntimeProviderId[]
  pendingAction: string | null
  disabled: boolean
  onEnable: (providerId: BrowserRuntimeProviderId) => void
  onSetFirst: (
    providerId: BrowserRuntimeProviderId,
    priority: BrowserRuntimeProviderId[],
  ) => void
}

function ProviderPriorityRow({
  row,
  priority,
  pendingAction,
  disabled,
  onEnable,
  onSetFirst,
}: ProviderPriorityRowProps): React.ReactElement {
  const enablePending = pendingAction === `enable:${row.lane.providerId}`
  const firstPending = pendingAction === `first:${row.lane.providerId}`
  const probeBlocked = row.lane.nextAction === 'run_probe'

  return (
    <div className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm font-medium">{row.lane.displayName}</span>
          <Badge variant={row.lane.routeRole === 'active' ? 'default' : 'outline'}>
            {row.statusLabel}
          </Badge>
          {row.isFirst ? <Badge variant="secondary">第一优先级</Badge> : null}
        </div>
        <div className="mt-1 text-xs text-muted-foreground">
          #{row.lane.priorityRank} · readiness {row.lane.readiness} · probe {row.lane.probeState}
        </div>
        {row.lane.providerId === 'browser.playwright_mcp' && !row.configureMcpClickable ? (
          <div className="mt-1 text-xs text-muted-foreground">
            MCP 配置将在 Kaleidoscope 集成分页接入，PR3 前不可点击。
          </div>
        ) : null}
        {probeBlocked ? (
          <div className="mt-1 text-xs text-muted-foreground">
            Probe gates wire in PR2.
          </div>
        ) : null}
      </div>
      <div className="flex min-h-11 flex-wrap items-center gap-2 md:justify-end">
        {row.canEnable ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || enablePending}
            onClick={() => onEnable(row.lane.providerId)}
          >
            <Power />
            {enablePending ? '启用中' : row.nextActionLabel}
          </Button>
        ) : (
          <Button type="button" variant="outline" size="sm" disabled>
            {row.nextActionLabel}
          </Button>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled || row.isFirst || firstPending}
          onClick={() => onSetFirst(row.lane.providerId, priority)}
        >
          <RotateCcw />
          {firstPending ? '更新中' : '设为第一'}
        </Button>
      </div>
    </div>
  )
}

function isDryRunAction(
  actionId: BrowserRuntimeSettingsAction['id'],
): actionId is BrowserRuntimePackAction {
  return DRY_RUN_ACTIONS.has(actionId)
}

function actionButtonLabel(action: BrowserRuntimeSettingsAction): string {
  if (isDryRunAction(action.id)) return `预览${action.label}`
  return action.label
}

function identityStatusLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  if (status.authorizedCount > 0) return '已授权'
  if (status.revokedCount > 0) return '已撤销'
  return '未连接'
}

function identityStatusDetail(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '等待身份状态。'
  if (status.authorizedCount > 0) {
    return `${status.authorizedCount} 个可用身份，${status.revokedCount} 个已撤销。`
  }
  if (status.revokedCount > 0) {
    return `${status.revokedCount} 个已撤销身份。`
  }
  return '未连接浏览器身份。'
}

function identityBadgeVariant(
  status: BrowserIdentityStatusReport | undefined,
): React.ComponentProps<typeof Badge>['variant'] {
  if (!status) return 'outline'
  if (status.authorizedCount > 0) return 'default'
  if (status.revokedCount > 0) return 'secondary'
  return 'outline'
}

function identityCountLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  return `${status.authorizedCount} 可用 / ${status.revokedCount} 已撤销`
}

function latestIdentityLastUsedLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  const latest = Math.max(
    ...status.profiles
      .map((profile) => profile.lastUsedAtMs ?? 0)
      .filter((timestamp) => timestamp > 0),
  )
  if (!Number.isFinite(latest) || latest <= 0) return '未知'
  return formatIdentityTimestamp(latest)
}

function identityActiveTaskLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  return `${status.activeTaskCount} 个任务`
}

function identityProfileStatusLabel(profile: BrowserIdentityProfileSummary): string {
  if (profile.revoked) return '已撤销'
  if (profile.status === 'live') return '可用'
  if (profile.status === 'stale') return '需刷新'
  return '未知'
}

function identityTaskStatusLabel(status: BrowserIdentityActiveTaskSummary['status']): string {
  switch (status) {
    case 'running':
      return '运行中'
    case 'completed':
      return '已完成'
    case 'failed':
      return '失败'
    case 'stopped':
      return '已停止'
    case 'needs_user_intervention':
      return '等待用户'
    case 'paused_waiting_for_browser_runtime':
      return '等待运行时'
    case 'paused_checkpointed':
      return '已检查点暂停'
    default:
      return status
  }
}

function identityProviderLabel(provider: BrowserIdentityProfileSummary['provider']): string {
  switch (provider) {
    case 'system_chrome':
      return 'System Chrome'
    case 'playwright':
      return 'Playwright'
    case 'browser_use':
      return 'Browser Use'
    case 'manual_import':
      return 'Manual import'
    default:
      return provider
  }
}

function identityScopeLabel(scope: BrowserIdentityProfileSummary['scope']): string {
  switch (scope) {
    case 'global':
      return 'Global'
    case 'workspace':
      return 'Workspace'
    case 'session':
      return 'Session'
    default:
      return scope
  }
}

function formatIdentityTimestamp(timestampMs: number | null): string {
  if (!timestampMs) return '未知'
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestampMs))
}

function badgeVariant(
  kind: ReturnType<typeof deriveBrowserRuntimeSettingsViewModel>['statusKind'],
): React.ComponentProps<typeof Badge>['variant'] {
  if (kind === 'ready') return 'default'
  if (kind === 'blocked') return 'destructive'
  if (kind === 'attention' || kind === 'deferred') return 'secondary'
  return 'outline'
}

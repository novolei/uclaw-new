import * as React from 'react'
import { useSetAtom } from 'jotai'
import {
  Activity,
  KeyRound,
  LogOut,
} from 'lucide-react'
import { kaleidoscopeModuleAtom, selectedBuiltinIntegrationAtom } from '@/atoms/kaleidoscope'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  deriveBrowserRuntimeControlCenterViewModel,
  priorityWithProviderFirst,
} from '@/lib/browser-runtime/browser-runtime-control-center'
import {
  deriveBrowserRuntimeSettingsViewModel,
  type BrowserRuntimeSettingsInput,
} from '@/lib/browser-runtime/browser-runtime-settings'
import { BrowserAutomationDiagnostics } from './browser-runtime/BrowserAutomationDiagnostics'
import { BrowserAutomationHeader } from './browser-runtime/BrowserAutomationHeader'
import { PlaywrightSetupProgress } from './browser-runtime/PlaywrightSetupProgress'
import { PlaywrightSkillsPanel } from './browser-runtime/PlaywrightSkillsPanel'
import { ProviderPriorityList } from './browser-runtime/ProviderPriorityList'
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimeProviderId,
} from '@/lib/startup/startup-doctor'
import {
  getBrowserRuntimeControlCenter,
  getBrowserRuntimeStatus,
  listBrowserIdentities,
  revokeBrowserIdentity,
  runBrowserRuntimeProviderProbe,
  runPlaywrightSetup,
  setBrowserRuntimeMcpRawToolsExposed,
  setBrowserRuntimeProviderEnabled,
  setBrowserRuntimeProviderPriority,
  type BrowserIdentityActiveTaskSummary,
  type BrowserIdentityProfileSummary,
  type BrowserIdentityStatusReport,
  type PlaywrightSetupExecutionReport,
} from '@/lib/tauri-bridge'
import { SettingsCard, SettingsRow, SettingsSection, SettingsToggle } from './primitives'

interface BrowserRuntimeSettingsProps {
  status?: BrowserRuntimeSettingsInput
}

export function BrowserRuntimeSettings({
  status,
}: BrowserRuntimeSettingsProps): React.ReactElement {
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setSelectedBuiltinIntegration = useSetAtom(selectedBuiltinIntegrationAtom)
  const [liveStatus, setLiveStatus] = React.useState<BrowserRuntimeSettingsInput | undefined>()
  const [identityStatus, setIdentityStatus] = React.useState<BrowserIdentityStatusReport | undefined>()
  const [revokingProfileId, setRevokingProfileId] = React.useState<string | null>(null)
  const [controlCenter, setControlCenter] = React.useState<BrowserRuntimeControlCenterReport | undefined>(
    status?.report?.controlCenter,
  )
  const [controlCenterError, setControlCenterError] = React.useState<string | undefined>()
  const [controlCenterPendingAction, setControlCenterPendingAction] = React.useState<string | null>(null)
  const [probePendingProviderId, setProbePendingProviderId] =
    React.useState<BrowserRuntimeProviderId | null>(null)
  const [setupReport, setSetupReport] = React.useState<PlaywrightSetupExecutionReport | undefined>()
  const [rawReportOpen, setRawReportOpen] = React.useState(false)
  const refreshGenerationRef = React.useRef(0)
  const identityGenerationRef = React.useRef(0)
  const controlCenterGenerationRef = React.useRef(0)
  const mountedRef = React.useRef(false)

  React.useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      refreshGenerationRef.current += 1
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

  const runProbe = React.useCallback(async (providerId: BrowserRuntimeProviderId) => {
    if (status || probePendingProviderId) return

    setProbePendingProviderId(providerId)
    setControlCenterError(undefined)
    try {
      await runBrowserRuntimeProviderProbe(providerId)
      await refreshControlCenter()
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setProbePendingProviderId(null)
      }
    }
  }, [probePendingProviderId, refreshControlCenter, status])

  const runSetup = React.useCallback(async () => {
    if (status || controlCenterPendingAction) return

    setControlCenterPendingAction('setup:auto')
    setControlCenterError(undefined)
    try {
      const report = await runPlaywrightSetup('auto_setup')
      if (mountedRef.current) {
        setSetupReport(report)
      }
      await refreshLiveStatus()
      await refreshControlCenter()
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [controlCenterPendingAction, refreshControlCenter, refreshLiveStatus, status])

  const setRawMcpToolsExposed = React.useCallback(async (exposed: boolean) => {
    if (status || controlCenterPendingAction) return

    setControlCenterPendingAction('mcp:raw-tools')
    setControlCenterError(undefined)
    try {
      const report = await setBrowserRuntimeMcpRawToolsExposed(exposed)
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
  }, [controlCenterPendingAction, status])

  const openPlaywrightMcpIntegration = React.useCallback(() => {
    setTopLevelView('kaleidoscope')
    setKaleidoscopeModule('integrations')
    setSelectedBuiltinIntegration('playwright_mcp')
  }, [setKaleidoscopeModule, setSelectedBuiltinIntegration, setTopLevelView])

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

  const model = deriveBrowserRuntimeSettingsViewModel(status ?? liveStatus)
  const activeControlCenter = controlCenter ?? status?.report?.controlCenter ?? liveStatus?.report?.controlCenter
  const controlModel = deriveBrowserRuntimeControlCenterViewModel(activeControlCenter)

  return (
    <div className="space-y-8">
      <BrowserAutomationHeader
        desiredLabel={controlModel.routeSummary.desiredLabel}
        activeLabel={controlModel.routeSummary.activeLabel}
        reasonLabel={controlModel.routeSummary.reasonLabel}
        primaryActionLabel={controlModel.routeSummary.primaryActionLabel}
        error={controlCenterError}
        disabled={Boolean(status)}
        onRefresh={() => {
          void refreshControlCenter()
        }}
      />

      <ProviderPriorityList
        rows={controlModel.providerRows}
        priority={activeControlCenter?.desiredProviderPriority ?? []}
        pendingAction={controlCenterPendingAction}
        probePendingProviderId={probePendingProviderId}
        disabled={Boolean(status)}
        onEnable={enableProvider}
        onSetFirst={setProviderFirst}
        onRunProbe={runProbe}
        onRunSetup={() => {
          void runSetup()
        }}
        onConfigureMcp={openPlaywrightMcpIntegration}
      />

      <PlaywrightSetupProgress
        statusLabel={controlModel.setupSummary.statusLabel}
        detailLabel={controlModel.setupSummary.detailLabel}
        needsNode={controlModel.setupSummary.needsNode}
        canAutoSetup={controlModel.setupSummary.canAutoSetup}
        pending={controlCenterPendingAction === 'setup:auto'}
        report={setupReport}
        onRunSetup={() => {
          void runSetup()
        }}
      />

      <PlaywrightSkillsPanel enabled={controlModel.setupSummary.statusLabel === 'Ready'} />

      <SettingsSection title="开发者 Guardrails" description="Advanced Browser Runtime controls">
        <SettingsCard>
          <SettingsToggle
            label="Expose raw Playwright MCP tools"
            description="默认关闭。开启后只把 uClaw allowlist 内的 Playwright MCP 原始工具暴露给 LLM；普通浏览器动作仍优先走 Browser Runtime Adapter。"
            checked={Boolean(activeControlCenter?.mcpIntegrationSummary.rawToolsExposed)}
            disabled={Boolean(status) || controlCenterPendingAction === 'mcp:raw-tools'}
            onCheckedChange={(checked) => {
              void setRawMcpToolsExposed(checked)
            }}
          />
        </SettingsCard>
      </SettingsSection>

      <BrowserAutomationDiagnostics
        report={activeControlCenter}
        model={controlModel}
        rawOpen={rawReportOpen}
        onToggleRaw={() => setRawReportOpen((open) => !open)}
      />

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

    </div>
  )
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

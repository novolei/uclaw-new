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
  deriveBrowserRuntimeSettingsViewModel,
  type BrowserRuntimeSettingsInput,
  type BrowserRuntimeSettingsAction,
} from '@/lib/browser-runtime/browser-runtime-settings'
import type { BrowserRuntimePackAction, BrowserRuntimePackExecutionReport } from '@/lib/startup/startup-doctor'
import {
  dryRunBrowserRuntimeAction,
  getBrowserRuntimeStatus,
  listBrowserIdentities,
  revokeBrowserIdentity,
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
  const [identityStatus, setIdentityStatus] = React.useState<BrowserIdentityStatusReport | undefined>()
  const [revokingProfileId, setRevokingProfileId] = React.useState<string | null>(null)
  const refreshGenerationRef = React.useRef(0)
  const dryRunGenerationRef = React.useRef(0)
  const identityGenerationRef = React.useRef(0)
  const mountedRef = React.useRef(false)

  React.useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      refreshGenerationRef.current += 1
      dryRunGenerationRef.current += 1
      identityGenerationRef.current += 1
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
      }
    } catch {
      // Keep the last displayed status when a manual refresh fails.
    }
  }, [status])

  const dryRunAction = React.useCallback(async (actionId: BrowserRuntimeSettingsAction['id']) => {
    if (status || !isDryRunAction(actionId)) return

    const generation = dryRunGenerationRef.current + 1
    dryRunGenerationRef.current = generation
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
    } catch {
      if (mountedRef.current && dryRunGenerationRef.current === generation) {
        setDryRunReports((current) => {
          const next = { ...current }
          delete next[actionId]
          return next
        })
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
      return
    }

    void refreshLiveStatus()
  }, [refreshLiveStatus, status])

  React.useEffect(() => {
    void refreshIdentityStatus()
  }, [refreshIdentityStatus])

  React.useEffect(() => {
    setDryRunReports({})
  }, [liveStatus, status])

  const model = deriveBrowserRuntimeSettingsViewModel(status ?? liveStatus)
  const [selectedActionId, setSelectedActionId] =
    React.useState<BrowserRuntimeSettingsAction['id'] | null>(null)
  const selectedAction =
    model.actions.find((action) => action.id === selectedActionId)
    ?? model.actions.find((action) => action.enabled)
  const selectedDryRunReport = selectedAction ? dryRunReports[selectedAction.id] : undefined

  return (
    <div className="space-y-8">
      <SettingsSection title="浏览器运行时" description="Playwright provider runtime pack">
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
                disabled={!action.enabled}
                aria-label={action.label}
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
                {action.label}
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
              description={selectedDryRunReport?.summary ?? selectedAction.preview.summary}
            >
              <Badge
                variant={
                  selectedDryRunReport?.destructive || selectedAction.preview.destructive
                    ? 'destructive'
                    : 'outline'
                }
              >
                {selectedDryRunReport
                  ? 'Dry run'
                  : selectedAction.preview.requiresConfirmation ? '需要确认' : '预览'}
              </Badge>
            </SettingsRow>
            <SettingsRow
              label="事件"
              description={(selectedDryRunReport?.eventNames ?? selectedAction.preview.eventNames).length > 0
                ? (selectedDryRunReport?.eventNames ?? selectedAction.preview.eventNames).join(' · ')
                : '等待后端事件接入'}
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

function isDryRunAction(
  actionId: BrowserRuntimeSettingsAction['id'],
): actionId is BrowserRuntimePackAction {
  return DRY_RUN_ACTIONS.has(actionId)
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
  if (status.activeTaskCount === null) return '等待任务状态'
  return `${status.activeTaskCount} 个任务`
}

function identityProfileStatusLabel(profile: BrowserIdentityProfileSummary): string {
  if (profile.revoked) return '已撤销'
  if (profile.status === 'live') return '可用'
  if (profile.status === 'stale') return '需刷新'
  return '未知'
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

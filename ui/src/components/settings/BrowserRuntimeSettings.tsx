import * as React from 'react'
import {
  Activity,
  Archive,
  Bug,
  Download,
  HardDrive,
  History,
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
import { getBrowserRuntimeStatus } from '@/lib/tauri-bridge'
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

export function BrowserRuntimeSettings({
  status,
}: BrowserRuntimeSettingsProps): React.ReactElement {
  const [liveStatus, setLiveStatus] = React.useState<BrowserRuntimeSettingsInput | undefined>()

  React.useEffect(() => {
    if (status) {
      setLiveStatus(undefined)
      return
    }

    let cancelled = false
    void getBrowserRuntimeStatus()
      .then((report) => {
        if (!cancelled) {
          setLiveStatus({
            report,
            lastCheckedAtMs: Date.now(),
          })
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLiveStatus(undefined)
        }
      })

    return () => {
      cancelled = true
    }
  }, [status])

  const model = deriveBrowserRuntimeSettingsViewModel(status ?? liveStatus)
  const [selectedActionId, setSelectedActionId] =
    React.useState<BrowserRuntimeSettingsAction['id'] | null>(null)
  const selectedAction =
    model.actions.find((action) => action.id === selectedActionId)
    ?? model.actions.find((action) => action.enabled)

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
          <SettingsRow label="运行时路径" description={model.runtimePackPathLabel} />
          <SettingsRow label="回滚" description={model.rollbackLabel} />
          <SettingsRow label="开发者回退" description={model.developerFallbackLabel} />
          <SettingsRow label="自动准备" description={model.autoPrepareLabel} />
        </SettingsCard>
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
                onClick={() => setSelectedActionId(action.id)}
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
              description={selectedAction.preview.summary}
            >
              <Badge variant={selectedAction.preview.destructive ? 'destructive' : 'outline'}>
                {selectedAction.preview.requiresConfirmation ? '需要确认' : '预览'}
              </Badge>
            </SettingsRow>
            <SettingsRow
              label="事件"
              description={selectedAction.preview.eventNames.length > 0
                ? selectedAction.preview.eventNames.join(' · ')
                : '等待后端事件接入'}
            >
              <span className="text-xs text-muted-foreground">无副作用</span>
            </SettingsRow>
          </SettingsCard>
        </SettingsSection>
      )}
    </div>
  )
}

function badgeVariant(
  kind: ReturnType<typeof deriveBrowserRuntimeSettingsViewModel>['statusKind'],
): React.ComponentProps<typeof Badge>['variant'] {
  if (kind === 'ready') return 'default'
  if (kind === 'blocked') return 'destructive'
  if (kind === 'attention' || kind === 'deferred') return 'secondary'
  return 'outline'
}

import * as React from 'react'
import { Clock3, Download, PauseCircle, PlayCircle, Settings } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type {
  BrowserRuntimeTaskTimePromptAction,
  BrowserRuntimeTaskTimePromptViewModel,
} from '@/lib/browser-runtime/browser-runtime-task-prompt'
import { cn } from '@/lib/utils'

interface BrowserRuntimeTaskTimePromptProps {
  model: BrowserRuntimeTaskTimePromptViewModel
  onAction?: (action: BrowserRuntimeTaskTimePromptAction) => void
  onOpenBrowserRuntimeSettings?: () => void
  className?: string
}

const ACTION_ICONS: Record<BrowserRuntimeTaskTimePromptAction['id'], React.ReactNode> = {
  prepare_now: <Download />,
  defer: <PauseCircle />,
  continue_without_browser: <PlayCircle />,
}

export function BrowserRuntimeTaskTimePrompt({
  model,
  onAction,
  onOpenBrowserRuntimeSettings,
  className,
}: BrowserRuntimeTaskTimePromptProps): React.ReactElement | null {
  const primaryAction = model.actions.find((action) => action.primary && action.enabled)

  if (!model.shouldShowPrompt) return null

  return (
    <section
      aria-label="浏览器运行时任务提示"
      className={cn('rounded-lg border bg-card text-card-foreground shadow-sm', className)}
    >
      <div className="space-y-4 p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0 space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant={badgeVariant(model.status)}>{statusLabel(model.status)}</Badge>
              {model.actions.some((action) => action.checkpointStatus) && (
                <Badge variant="secondary">将暂停任务</Badge>
              )}
            </div>
            <div>
              <h2 className="text-base font-semibold leading-6">{model.title}</h2>
              <p className="mt-1 text-sm leading-6 text-muted-foreground">{model.summary}</p>
            </div>
          </div>

          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Clock3 size={14} />
            任务时间确认
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-3">
          {model.actions.map((action) => (
            <button
              key={action.id}
              type="button"
              disabled={!action.enabled}
              aria-label={action.label}
              aria-pressed={primaryAction?.id === action.id}
              onClick={() => onAction?.(action)}
              className={cn(
                'min-h-[132px] rounded-lg border bg-background p-3 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
                action.enabled && 'hover:bg-accent hover:text-accent-foreground',
                action.primary && action.enabled && 'border-primary shadow-sm',
                !action.enabled && 'cursor-not-allowed opacity-55',
              )}
            >
              <span className="flex items-start justify-between gap-3">
                <span className="flex min-w-0 items-center gap-2 text-sm font-medium">
                  <span className="[&_svg]:size-4 [&_svg]:shrink-0">{ACTION_ICONS[action.id]}</span>
                  <span>{action.label}</span>
                </span>
                {action.primary && action.enabled && <Badge>推荐</Badge>}
              </span>
              <span className="mt-3 block text-sm leading-5 text-muted-foreground">
                {action.summary}
              </span>
              {action.checkpointStatus && (
                <span className="mt-3 block text-xs font-medium text-muted-foreground">
                  checkpoint: {action.checkpointStatus}
                </span>
              )}
            </button>
          ))}
        </div>

        <div className="flex flex-col gap-3 rounded-lg border bg-muted/35 p-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <p className="text-xs font-medium text-muted-foreground">事件预览</p>
            <p className="mt-1 break-words text-xs leading-5 text-muted-foreground">
              {eventPreview(model.actions)}
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap gap-2">
            {onOpenBrowserRuntimeSettings ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="gap-2"
                onClick={onOpenBrowserRuntimeSettings}
              >
                <Settings aria-hidden className="h-3.5 w-3.5" />
                Browser Runtime Settings
              </Button>
            ) : null}
            <Button
              type="button"
              size="sm"
              disabled={!primaryAction}
              aria-label={primaryAction ? `推荐操作：${primaryAction.label}` : '等待推荐操作'}
              onClick={() => primaryAction && onAction?.(primaryAction)}
            >
              {primaryAction ? ACTION_ICONS[primaryAction.id] : <Clock3 />}
              {primaryAction?.label ?? '等待选择'}
            </Button>
          </div>
        </div>
      </div>
    </section>
  )
}

function badgeVariant(
  status: BrowserRuntimeTaskTimePromptViewModel['status'],
): React.ComponentProps<typeof Badge>['variant'] {
  if (status === 'blocked') return 'destructive'
  if (status === 'confirmation_required' || status === 'deferred') return 'secondary'
  return 'outline'
}

function statusLabel(status: BrowserRuntimeTaskTimePromptViewModel['status']): string {
  switch (status) {
    case 'ready':
      return '可用'
    case 'confirmation_required':
      return '需要确认'
    case 'deferred':
      return '已推迟'
    case 'blocked':
      return '受阻'
    case 'prepare_required':
    default:
      return '待准备'
  }
}

function eventPreview(actions: BrowserRuntimeTaskTimePromptAction[]): string {
  const names = Array.from(new Set(actions.flatMap((action) => action.eventNames)))
  return names.length > 0 ? names.join(' · ') : '等待后续 TaskEvent 接入'
}

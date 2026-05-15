import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { toggleArchiveAgentSession } from '@/lib/tauri-bridge'

interface Props {
  activity: AutomationActivity
  onOpenRunSession?: (sessionId: string) => void
  onArchived?: (sessionId: string) => void
}

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  completed:    { label: '已完成', className: 'text-success' },
  failed:       { label: '失败',   className: 'text-danger' },
  cancelled:    { label: '已取消', className: 'text-muted-foreground' },
  filtered_out: { label: '已跳过', className: 'text-muted-foreground' },
  waiting_user: { label: '待确认', className: 'text-warning' },
  running:      { label: '运行中', className: 'text-primary' },
  queued:       { label: '排队中', className: 'text-muted-foreground' },
}

function formatTs(ms: number | null): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

export function ActivityListItem({ activity, onOpenRunSession, onArchived }: Props) {
  const [archiving, setArchiving] = useState(false)
  const cfg = STATUS_CONFIG[activity.status] ?? { label: activity.status, className: 'text-muted-foreground' }
  const isEscalation = activity.status === 'waiting_user'

  async function handleArchive() {
    if (!activity.sessionId || archiving) return
    setArchiving(true)
    try {
      await toggleArchiveAgentSession(activity.sessionId)
      onArchived?.(activity.sessionId)
    } finally {
      setArchiving(false)
    }
  }

  return (
    <div
      data-testid={`activity-row-${activity.id}`}
      className={[
        'group rounded-lg border p-3 bg-background',
        isEscalation ? 'border-warning ring-1 ring-warning/20' : 'border-border/50',
      ].join(' ')}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{formatTs(activity.startedAt ?? activity.queuedAt)}</span>
          <span className={cfg.className}>{cfg.label}</span>
          {activity.durationMs > 0 && (
            <span>{formatDuration(activity.durationMs)}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {activity.sessionId && (
            <button
              onClick={handleArchive}
              disabled={archiving}
              className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
              aria-label="归档"
            >
              归档
            </button>
          )}
          {activity.sessionId && (
            <button
              onClick={() => onOpenRunSession?.(activity.sessionId!)}
              className="titlebar-no-drag text-xs text-primary hover:underline shrink-0"
            >
              查看进程 &gt;
            </button>
          )}
        </div>
      </div>
      {activity.reportText && (
        <p className="mt-1 text-sm text-foreground line-clamp-3">{activity.reportText}</p>
      )}
    </div>
  )
}

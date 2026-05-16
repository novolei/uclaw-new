import { useState } from 'react'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { ActivityListItem } from './ActivityListItem'
import { RunSessionSubView } from './RunSessionSubView'

interface Props {
  specId: string
  activities: AutomationActivity[]
  onOpenRunSession?: (sessionId: string) => void
  activeRunSessionId?: string | null
  onCloseRunSession?: () => void
}

function dotColorClass(activity: AutomationActivity): string {
  const { status, reportOutcome } = activity
  if (status === 'running' || status === 'queued')
    return 'bg-primary animate-pulse'
  if (status === 'failed' || reportOutcome === 'error')
    return 'bg-danger'
  if (status === 'waiting_user')
    return 'bg-warning'
  if (status === 'cancelled')
    return 'bg-muted-foreground'
  if (status === 'completed') {
    if (reportOutcome === 'useful') return 'bg-success'
    return 'bg-muted-foreground'
  }
  return 'bg-muted-foreground'
}

export function ActivityHistoryView({
  specId: _specId,
  activities,
  onOpenRunSession,
  activeRunSessionId,
  onCloseRunSession,
}: Props) {
  const [archivedIds, setArchivedIds] = useState<Set<string>>(new Set())
  const [showArchived, setShowArchived] = useState(false)

  if (activeRunSessionId) {
    const activeActivity = activities.find((a) => a.sessionId === activeRunSessionId)
    const isRunning =
      activeActivity?.status === 'running' || activeActivity?.status === 'queued'
    return (
      <RunSessionSubView
        sessionId={activeRunSessionId}
        isRunning={isRunning}
        activity={activeActivity ?? null}
        onBack={() => onCloseRunSession?.()}
      />
    )
  }

  function handleArchived(sessionId: string) {
    setArchivedIds((prev) => new Set([...prev, sessionId]))
  }

  const visible = showArchived
    ? activities
    : activities.filter((a) => !a.sessionId || !archivedIds.has(a.sessionId))

  if (activities.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        还没有运行记录
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      {archivedIds.size > 0 && (
        <div className="px-3 pt-2 shrink-0">
          <button
            onClick={() => setShowArchived((v) => !v)}
            className="titlebar-no-drag text-xs text-muted-foreground hover:text-foreground"
            aria-label={showArchived ? '隐藏已归档' : '显示已归档'}
          >
            {showArchived ? '隐藏已归档' : `显示已归档 (${archivedIds.size})`}
          </button>
        </div>
      )}

      {/* Timeline list */}
      <div className="flex-1 flex flex-col overflow-y-auto px-3 pt-3 pb-1">
        {visible.map((act, idx) => (
          <div key={act.id} className="flex gap-3">
            {/* Dot + connector */}
            <div className="flex flex-col items-center w-3 shrink-0 pt-1.5">
              <div className={`w-2.5 h-2.5 rounded-full shrink-0 ${dotColorClass(act)}`} />
              {idx < visible.length - 1 && (
                <div
                  className="w-px flex-1 bg-border/30 mt-1"
                  style={{ minHeight: '1.5rem' }}
                />
              )}
            </div>
            {/* Card */}
            <div className="flex-1 pb-3 group min-w-0">
              <ActivityListItem
                activity={act}
                onOpenRunSession={onOpenRunSession}
                onArchived={handleArchived}
              />
            </div>
          </div>
        ))}
        {visible.length === 0 && (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
            所有记录已归档
          </div>
        )}
      </div>
    </div>
  )
}

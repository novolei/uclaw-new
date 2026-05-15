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

export function ActivityHistoryView({
  specId: _specId,
  activities,
  onOpenRunSession,
  activeRunSessionId,
  onCloseRunSession,
}: Props) {
  // Local tracking: session IDs archived in this session. Avoids a backend
  // query change — items filtered here reappear in the next full reload.
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
      <div className="flex-1 flex flex-col gap-2 p-3 overflow-y-auto">
        {visible.map((act) => (
          <ActivityListItem
            key={act.id}
            activity={act}
            onOpenRunSession={onOpenRunSession}
            onArchived={handleArchived}
          />
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

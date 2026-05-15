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
  if (activeRunSessionId) {
    return (
      <RunSessionSubView
        sessionId={activeRunSessionId}
        onBack={() => onCloseRunSession?.()}
      />
    )
  }

  if (activities.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        还没有运行记录
      </div>
    )
  }

  return (
    <div className="flex-1 flex flex-col gap-2 p-3 overflow-y-auto">
      {activities.map((act) => (
        <ActivityListItem
          key={act.id}
          activity={act}
          onOpenRunSession={onOpenRunSession}
        />
      ))}
    </div>
  )
}

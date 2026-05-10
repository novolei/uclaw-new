import * as React from 'react'
import { useAtomValue } from 'jotai'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { SessionItem } from './SessionItem'
import { agentSessionIndicatorMapAtom } from '@/atoms/agent-atoms'
import type { WorkspaceSession } from '@/atoms/workspace'

interface WorkspaceGroupProps {
  id: string
  name: string
  icon: string
  sessions: WorkspaceSession[]
  isActive: boolean
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
  onSelectWorkspace: () => void
}

export function WorkspaceGroup({
  name,
  icon,
  sessions,
  isActive,
  activeSessionId,
  onSelectSession,
  onDeleteSession,
  onSelectWorkspace,
}: WorkspaceGroupProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(isActive)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)

  React.useEffect(() => {
    if (isActive) setExpanded(true)
  }, [isActive])

  return (
    <div className="mb-1">
      <div
        className={cn(
          'flex items-center gap-1.5 px-2 py-1 rounded-md cursor-pointer select-none',
          'text-[12px] font-semibold uppercase tracking-wide',
          isActive
            ? 'text-foreground'
            : 'text-muted-foreground hover:text-foreground'
        )}
        onClick={() => {
          onSelectWorkspace()
          setExpanded((v) => !v)
        }}
      >
        <span className="text-[13px]">{icon}</span>
        <span className="flex-1 truncate">{name}</span>
        {expanded ? (
          <ChevronDown className="h-3 w-3 shrink-0" />
        ) : (
          <ChevronRight className="h-3 w-3 shrink-0" />
        )}
      </div>
      {expanded && (
        <div className="pl-3 flex flex-col gap-0.5 mt-0.5">
          {sessions.length === 0 && (
            <p className="text-[11px] text-muted-foreground px-2 py-1">No sessions yet</p>
          )}
          {sessions.map((s) => (
            <SessionItem
              key={s.id}
              id={s.id}
              title={s.title}
              titleEmoji={s.titleEmoji}
              titlePending={s.titlePending}
              isActive={activeSessionId === s.id}
              running={indicatorMap.get(s.id) === 'running'}
              onClick={() => onSelectSession(s.id)}
              onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            />
          ))}
        </div>
      )}
    </div>
  )
}

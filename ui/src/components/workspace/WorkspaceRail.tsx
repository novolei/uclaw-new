import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Plus } from 'lucide-react'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  refreshWorkspacesAtom,
  selectWorkspaceAtom,
} from '@/atoms/workspace'
import { WorkspaceGroup } from './WorkspaceGroup'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'
import { cn } from '@/lib/utils'

interface WorkspaceRailProps {
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
}

export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
}: WorkspaceRailProps): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const workspaceSessions = useAtomValue(workspaceSessionsAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const [createOpen, setCreateOpen] = React.useState(false)

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const handleCreated = async (ws: { id: string; name: string; icon: string }) => {
    await refreshWorkspaces()
    await selectWorkspace(ws.id)
  }

  return (
    <div className="flex flex-col h-full w-full">
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {workspaces.map((ws) => (
          <WorkspaceGroup
            key={ws.id}
            id={ws.id}
            name={ws.name}
            icon={ws.icon}
            sessions={workspaceSessions[ws.id] ?? []}
            isActive={activeWorkspaceId === ws.id}
            activeSessionId={activeSessionId}
            onSelectSession={onSelectSession}
            onDeleteSession={onDeleteSession}
            onSelectWorkspace={() => selectWorkspace(ws.id)}
          />
        ))}
      </div>
      <div className="px-3 pb-2">
        <button
          onClick={() => setCreateOpen(true)}
          className={cn(
            'flex items-center gap-2 w-full px-3 py-1.5 rounded-[10px]',
            'text-[12px] text-foreground/40 hover:text-foreground/70 hover:bg-primary/5',
            'transition-colors duration-100 titlebar-no-drag'
          )}
        >
          <Plus className="h-3.5 w-3.5" />
          新建工作区
        </button>
      </div>
      <WorkspaceCreateDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={handleCreated}
      />
    </div>
  )
}

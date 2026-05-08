import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Plus, Settings } from 'lucide-react'
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
  onOpenSettings?: () => void
}

export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
  onOpenSettings,
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
    <div className="flex flex-col h-full w-[220px] shrink-0">
      <div className="flex items-center justify-between px-3 py-3">
        <span className="text-sm font-semibold text-foreground">uClaw</span>
        <button
          onClick={onOpenSettings}
          className="text-muted-foreground hover:text-foreground rounded p-1"
        >
          <Settings className="h-4 w-4" />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-1">
        <p className="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground px-2 mb-1">
          Workspaces
        </p>
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
      <div className="p-2 border-t border-border/50">
        <button
          onClick={() => setCreateOpen(true)}
          className={cn(
            'flex items-center gap-2 w-full px-2 py-1.5 rounded-md',
            'text-[12px] text-muted-foreground hover:text-foreground hover:bg-muted',
            'transition-colors duration-100'
          )}
        >
          <Plus className="h-3.5 w-3.5" />
          New Workspace
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

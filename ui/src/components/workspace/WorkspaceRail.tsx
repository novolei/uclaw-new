import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Plus } from 'lucide-react'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  refreshWorkspacesAtom,
  selectWorkspaceAtom,
  reorderWorkspacesAtom,
} from '@/atoms/workspace'
import { WorkspaceGroup } from './WorkspaceGroup'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'
import { MoveSessionDialog } from '@/components/agent/MoveSessionDialog'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
import type { AgentWorkspace } from '@/lib/agent-types'
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
  const reorderWorkspaces = useSetAtom(reorderWorkspacesAtom)
  const [createOpen, setCreateOpen] = React.useState(false)

  const [dragId, setDragId] = React.useState<string | null>(null)
  const [dropIndicator, setDropIndicator] = React.useState<{ id: string; position: 'before' | 'after' } | null>(null)

  // Move-session dialog state.
  const [moveTargetSessionId, setMoveTargetSessionId] = React.useState<string | null>(null)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const moveTargetSession = moveTargetSessionId
    ? agentSessions.find((s) => s.id === moveTargetSessionId)
    : null
  const agentWorkspaces: AgentWorkspace[] = React.useMemo(
    () => workspaces.map((w) => ({
      id: w.id,
      name: w.name,
      icon: w.icon,
      path: w.path,
      createdAt: Date.parse(w.createdAt) || Date.now(),
      updatedAt: Date.parse(w.updatedAt) || Date.now(),
    })),
    [workspaces],
  )

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const handleCreated = async (ws: { id: string; name: string; icon: string }) => {
    await refreshWorkspaces()
    await selectWorkspace(ws.id)
  }

  const handleDragStart = (e: React.DragEvent, id: string): void => {
    console.debug('[workspace-dnd] dragstart', { id })
    setDragId(id)
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', id)
  }

  const handleDragOver = (e: React.DragEvent, targetId: string): void => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    if (!dragId || dragId === targetId) {
      setDropIndicator(null)
      return
    }
    const rect = e.currentTarget.getBoundingClientRect()
    const ratio = (e.clientY - rect.top) / rect.height
    const position: 'before' | 'after' = ratio < 0.5 ? 'before' : 'after'
    if (dropIndicator?.id === targetId && dropIndicator.position === position) return
    setDropIndicator({ id: targetId, position })
  }

  const handleDragLeave = (e: React.DragEvent): void => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setDropIndicator(null)
    }
  }

  const handleDrop = async (e: React.DragEvent, targetId: string): Promise<void> => {
    e.preventDefault()
    e.stopPropagation()
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const ratio = (e.clientY - rect.top) / rect.height
    const position: 'before' | 'after' = ratio < 0.5 ? 'before' : 'after'
    const sourceId = dragId ?? e.dataTransfer.getData('text/plain') ?? ''
    console.debug('[workspace-dnd] drop', { sourceId, targetId, position, dragIdState: dragId })
    setDragId(null)
    setDropIndicator(null)
    if (!sourceId || sourceId === targetId) {
      console.debug('[workspace-dnd] drop bailed', { sourceId, targetId })
      return
    }
    const fromIdx = workspaces.findIndex((w) => w.id === sourceId)
    const toIdx = workspaces.findIndex((w) => w.id === targetId)
    if (fromIdx === -1 || toIdx === -1) {
      console.warn('[workspace-dnd] index lookup failed', { fromIdx, toIdx, sourceId, targetId })
      return
    }
    const reordered = [...workspaces]
    const [moved] = reordered.splice(fromIdx, 1)
    const adjustedToIdx = fromIdx < toIdx ? toIdx - 1 : toIdx
    const insertIdx = position === 'after' ? adjustedToIdx + 1 : adjustedToIdx
    reordered.splice(insertIdx, 0, moved!)
    const newOrder = reordered.map((w) => w.id)
    console.debug('[workspace-dnd] calling reorderWorkspaces', { newOrder })
    try {
      await reorderWorkspaces(newOrder)
      console.debug('[workspace-dnd] reorderWorkspaces succeeded')
    } catch (err) {
      console.error('[workspace-dnd] reorder failed', err)
    }
  }

  const handleDragEnd = (): void => {
    setDragId(null)
    setDropIndicator(null)
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
            onMoveSession={(sid) => setMoveTargetSessionId(sid)}
            onSelectWorkspace={() => selectWorkspace(ws.id)}
            isDragging={dragId === ws.id}
            dropIndicator={dropIndicator?.id === ws.id ? dropIndicator.position : null}
            onDragStart={handleDragStart}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
            onDragEnd={handleDragEnd}
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
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={agentWorkspaces}
          onMoved={() => {
            setMoveTargetSessionId(null)
            void refreshWorkspaces()
          }}
        />
      )}
    </div>
  )
}

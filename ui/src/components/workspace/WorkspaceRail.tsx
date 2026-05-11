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

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const handleCreated = async (ws: { id: string; name: string; icon: string }) => {
    await refreshWorkspaces()
    await selectWorkspace(ws.id)
  }

  const handleDragStart = (e: React.DragEvent, id: string): void => {
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

  const handleDrop = async (_e: React.DragEvent, targetId: string): Promise<void> => {
    if (!dragId || dragId === targetId || !dropIndicator) {
      setDragId(null)
      setDropIndicator(null)
      return
    }
    const fromIdx = workspaces.findIndex((w) => w.id === dragId)
    const toIdx = workspaces.findIndex((w) => w.id === targetId)
    if (fromIdx === -1 || toIdx === -1) {
      setDragId(null)
      setDropIndicator(null)
      return
    }
    const reordered = [...workspaces]
    const [moved] = reordered.splice(fromIdx, 1)
    const adjustedToIdx = fromIdx < toIdx ? toIdx - 1 : toIdx
    const insertIdx = dropIndicator.position === 'after' ? adjustedToIdx + 1 : adjustedToIdx
    reordered.splice(insertIdx, 0, moved!)
    setDragId(null)
    setDropIndicator(null)
    try {
      await reorderWorkspaces(reordered.map((w) => w.id))
    } catch (err) {
      console.error('[workspace] reorder failed', err)
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
    </div>
  )
}

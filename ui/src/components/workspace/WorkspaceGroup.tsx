import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { ChevronRight, ChevronDown, Pencil, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { SessionItem } from './SessionItem'
import { agentSessionIndicatorMapAtom } from '@/atoms/agent-atoms'
import {
  updateWorkspaceAtom,
  refreshWorkspacesAtom,
  type WorkspaceSession,
} from '@/atoms/workspace'
import { deleteWorkspace } from '@/lib/tauri-bridge'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

interface WorkspaceGroupProps {
  id: string
  name: string
  icon: string
  sessions: WorkspaceSession[]
  isActive: boolean
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
  onMoveSession?: (sessionId: string) => void
  onSelectWorkspace: () => void
  /** Whether this row is currently being dragged. */
  isDragging?: boolean
  /** Drop indicator: 'before' | 'after' | null. */
  dropIndicator?: 'before' | 'after' | null
  /** DnD handlers from parent (WorkspaceRail). */
  onDragStart?: (e: React.DragEvent, id: string) => void
  onDragOver?: (e: React.DragEvent, id: string) => void
  onDragLeave?: (e: React.DragEvent) => void
  onDrop?: (e: React.DragEvent, id: string) => void
  onDragEnd?: () => void
}

export function WorkspaceGroup({
  id,
  name,
  icon,
  sessions,
  isActive,
  activeSessionId,
  onSelectSession,
  onDeleteSession,
  onMoveSession,
  onSelectWorkspace,
  isDragging,
  dropIndicator,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
}: WorkspaceGroupProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(isActive)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const updateWs = useSetAtom(updateWorkspaceAtom)
  const refreshWs = useSetAtom(refreshWorkspacesAtom)

  const [renaming, setRenaming] = React.useState(false)
  const [renameValue, setRenameValue] = React.useState(name)
  const renameInputRef = React.useRef<HTMLInputElement>(null)
  const [confirmingDelete, setConfirmingDelete] = React.useState(false)

  React.useEffect(() => {
    if (isActive) setExpanded(true)
  }, [isActive])

  React.useEffect(() => {
    if (renaming) {
      requestAnimationFrame(() => {
        renameInputRef.current?.focus()
        renameInputRef.current?.select()
      })
    }
  }, [renaming])

  const canMutate = id !== 'default'

  const startRename = (e: React.MouseEvent): void => {
    e.stopPropagation()
    setRenameValue(name)
    setRenaming(true)
  }

  const commitRename = async (): Promise<void> => {
    const trimmed = renameValue.trim()
    if (!trimmed || trimmed === name) {
      setRenaming(false)
      return
    }
    try {
      await updateWs({ id, name: trimmed })
    } catch (err) {
      const msg = err instanceof Error ? err.message : '重命名失败'
      toast.error(msg)
    } finally {
      setRenaming(false)
    }
  }

  const cancelRename = (): void => {
    setRenaming(false)
    setRenameValue(name)
  }

  const handleRenameKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      if (e.nativeEvent.isComposing) return
      e.preventDefault()
      commitRename()
    } else if (e.key === 'Escape') {
      cancelRename()
    }
  }

  const confirmDelete = async (): Promise<void> => {
    try {
      await deleteWorkspace(id)
      await refreshWs()
    } catch (err) {
      const msg = err instanceof Error ? err.message : '删除失败'
      toast.error(msg)
    } finally {
      setConfirmingDelete(false)
    }
  }

  return (
    <>
      <div className="mb-1 relative">
        {dropIndicator === 'before' && (
          <div className="absolute -top-0.5 left-1 right-1 h-0.5 bg-primary rounded-full z-10" />
        )}
        <div
          draggable={canMutate && !renaming}
          onDragStart={(e) => onDragStart?.(e, id)}
          onDragOver={(e) => onDragOver?.(e, id)}
          onDragLeave={onDragLeave}
          onDrop={(e) => onDrop?.(e, id)}
          onDragEnd={onDragEnd}
          className={cn(
            'group flex items-center gap-1.5 px-2 py-1 rounded-md cursor-pointer select-none',
            'text-[12px] font-semibold uppercase tracking-wide',
            isActive ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
            isDragging && 'opacity-40',
          )}
          onClick={() => {
            if (renaming) return
            onSelectWorkspace()
            setExpanded((v) => !v)
          }}
        >
          <span className="text-[13px]">{icon}</span>
          {renaming ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={commitRename}
              onClick={(e) => e.stopPropagation()}
              className="flex-1 min-w-0 bg-transparent text-[12px] uppercase tracking-wide border-b border-primary/50 outline-none px-0.5"
              maxLength={64}
            />
          ) : (
            <span className="flex-1 truncate">{name}</span>
          )}

          {canMutate && !renaming && (
            <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
              <button
                onClick={startRename}
                className="p-0.5 rounded hover:bg-foreground/[0.08] text-foreground/30 hover:text-foreground/60 transition-colors"
                title="重命名"
              >
                <Pencil className="size-3" />
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); setConfirmingDelete(true) }}
                className="p-0.5 rounded hover:bg-destructive/10 text-foreground/30 hover:text-destructive transition-colors"
                title="删除"
              >
                <Trash2 className="size-3" />
              </button>
            </div>
          )}

          {!renaming && (expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0" />
          ))}
        </div>
        {dropIndicator === 'after' && (
          <div className="absolute -bottom-0.5 left-1 right-1 h-0.5 bg-primary rounded-full z-10" />
        )}
      </div>

      {expanded && (
        <div className="pl-3 flex flex-col gap-0.5 mt-0.5 mb-1">
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
              onMove={onMoveSession ? () => onMoveSession(s.id) : undefined}
            />
          ))}
        </div>
      )}

      <AlertDialog open={confirmingDelete} onOpenChange={(v) => { if (!v) setConfirmingDelete(false) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除工作区?</AlertDialogTitle>
            <AlertDialogDescription>
              删除「{name}」后,该工作区下的会话会被移动到「默认工作区」。文件夹本身不会被删除。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

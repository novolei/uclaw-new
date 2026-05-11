/**
 * WorkspaceHeader — top-of-sidebar element showing the active
 * workspace's emoji + name + truncated path with hover ✏ rename + 🗑
 * delete buttons.
 *
 * Phase 4b (ARC-style switcher): replaces the per-workspace header
 * that used to live inside the workspace tree. The tree itself now
 * shows only the active workspace's sessions.
 *
 * Default workspace shows the read-only view (canMutate=false). All
 * other workspaces show hover-buttons on the right.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Pencil, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  updateWorkspaceAtom,
  selectWorkspaceAtom,
  refreshWorkspacesAtom,
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
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { IconPicker } from './IconPicker'
import { getWorkspaceIcon } from '@/lib/workspace-icons'

/**
 * Replace the leading /Users/<name> or /home/<name> with ~ for display.
 * Best-effort: we can't read $HOME from the renderer, so we pattern-match
 * the macOS/Linux conventions. Returns the path unchanged on no match.
 */
function withTilde(path: string): string {
  const m = path.match(/^(?:\/Users\/[^/]+|\/home\/[^/]+)\/(.*)$/)
  return m ? `~/${m[1]}` : path
}

export function WorkspaceHeader(): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const updateWs = useSetAtom(updateWorkspaceAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const refreshWs = useSetAtom(refreshWorkspacesAtom)

  const [renaming, setRenaming] = React.useState(false)
  const [renameValue, setRenameValue] = React.useState('')
  const [confirmingDelete, setConfirmingDelete] = React.useState(false)
  const renameInputRef = React.useRef<HTMLInputElement>(null)

  // All hooks must come before any conditional return.
  React.useEffect(() => {
    if (renaming) {
      requestAnimationFrame(() => {
        renameInputRef.current?.focus()
        renameInputRef.current?.select()
      })
    }
  }, [renaming])

  const active = workspaces.find((w) => w.id === activeId)
  if (!active) return null

  const canMutate = active.id !== 'default'
  const displayPath = active.path ? withTilde(active.path) : null

  const startRename = (): void => {
    setRenameValue(active.name)
    setRenaming(true)
  }

  const commitRename = async (): Promise<void> => {
    const trimmed = renameValue.trim()
    if (!trimmed || trimmed === active.name) {
      setRenaming(false)
      return
    }
    try {
      await updateWs({ id: active.id, name: trimmed })
    } catch (err) {
      const msg = err instanceof Error ? err.message : '重命名失败'
      toast.error(msg)
    } finally {
      setRenaming(false)
    }
  }

  const cancelRename = (): void => {
    setRenaming(false)
    setRenameValue(active.name)
  }

  const handleRenameKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      if (e.nativeEvent.isComposing) return
      e.preventDefault()
      void commitRename()
    } else if (e.key === 'Escape') {
      cancelRename()
    }
  }

  const confirmDelete = async (): Promise<void> => {
    try {
      await deleteWorkspace(active.id)
      // After delete: backend re-homes orphan sessions to 'default'.
      // Frontend switches active to 'default' and refreshes the list.
      await selectWorkspace('default')
      await refreshWs()
    } catch (err) {
      const msg = err instanceof Error ? err.message : '删除失败'
      toast.error(msg)
    } finally {
      setConfirmingDelete(false)
    }
  }

  const ActiveIcon = getWorkspaceIcon(active.icon)
  const handlePickIcon = async (iconName: string): Promise<void> => {
    if (iconName === active.icon) return
    try {
      await updateWs({ id: active.id, icon: iconName })
    } catch (err) {
      const msg = err instanceof Error ? err.message : '更换图标失败'
      toast.error(msg)
    }
  }

  return (
    <>
      <div className="group flex items-center gap-2 px-3 py-2 mx-3 mt-1 rounded-md
                      hover:bg-foreground/[0.03] transition-colors">
        {canMutate ? (
          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label="更换图标"
                title="更换图标"
                className="flex-shrink-0 inline-flex items-center justify-center
                           size-7 rounded-md bg-primary/10 text-primary
                           hover:bg-primary/20 transition-colors"
              >
                <ActiveIcon className="size-4" aria-hidden />
              </button>
            </PopoverTrigger>
            <PopoverContent align="start" sideOffset={6} className="w-auto p-2 z-[100]">
              <IconPicker value={active.icon} onChange={(v) => void handlePickIcon(v)} />
            </PopoverContent>
          </Popover>
        ) : (
          <div
            className="flex-shrink-0 inline-flex items-center justify-center
                       size-7 rounded-md bg-primary/10 text-primary"
            aria-label={`图标: ${active.icon}`}
          >
            <ActiveIcon className="size-4" aria-hidden />
          </div>
        )}
        <div className="flex-1 min-w-0">
          {renaming ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={() => void commitRename()}
              className="w-full bg-transparent text-[13px] font-semibold
                         border-b border-primary/50 outline-none px-0"
              maxLength={64}
            />
          ) : (
            <div className="text-[13px] font-semibold truncate" title={active.name}>
              {active.name}
            </div>
          )}
          {displayPath && !renaming && (
            <div className="text-[10px] text-muted-foreground/70 truncate font-mono"
                 title={active.path ?? undefined}>
              {displayPath}
            </div>
          )}
        </div>
        {canMutate && !renaming && (
          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100
                          transition-opacity flex-shrink-0">
            <button
              type="button"
              onClick={startRename}
              className="p-1 rounded text-foreground/40 hover:text-foreground/70
                         hover:bg-foreground/[0.06] transition-colors"
              title="重命名"
            >
              <Pencil className="size-3.5" />
            </button>
            <button
              type="button"
              onClick={() => setConfirmingDelete(true)}
              className="p-1 rounded text-foreground/40 hover:text-destructive
                         hover:bg-destructive/10 transition-colors"
              title="删除工作区"
            >
              <Trash2 className="size-3.5" />
            </button>
          </div>
        )}
      </div>

      <AlertDialog
        open={confirmingDelete}
        onOpenChange={(v) => { if (!v) setConfirmingDelete(false) }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除工作区?</AlertDialogTitle>
            <AlertDialogDescription>
              删除「{active.name}」后,该工作区下的会话会被移动到「默认工作区」。
              文件夹本身不会被删除。
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

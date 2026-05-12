/**
 * FileRowMenu — 3-dot hover menu for a single FileTreeNode row.
 *
 * Five actions surface via shadcn DropdownMenu:
 *   1. 添加到聊天 (files only)            — addPendingAttachmentAction
 *   2. 在文件夹中显示                     — reveal_path_in_file_manager
 *   3. 移动到…   (workspace-mount only)  — moveTargetAtom
 *   4. 重命名     (workspace-mount only)  — renamingFilePathAtom
 *   5. 删除       (workspace-mount only)  — deleteTargetAtom
 *
 * Items 3/4/5 render but disabled on non-workspace mounts (clear UX
 * over hidden actions). Backend rename/move/delete commands resolve
 * paths under <data_dir>/spaces/<sid>/workspace/ only — they literally
 * cannot operate on attached or session paths.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import {
  MoreHorizontal,
  MessageSquarePlus,
  FolderSearch,
  FolderInput,
  Pencil,
  Trash2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'
import {
  renamingFilePathAtom,
  moveTargetAtom,
  deleteTargetAtom,
} from '@/atoms/files-rail-row-atoms'
import type { MountRoot } from '@/atoms/files-rail-atoms'

interface Props {
  mount: MountRoot
  sessionId: string | null
  relPath: string
  name: string
  isDirectory: boolean
  absolutePath: string
}

const READONLY_TOOLTIP = '只读 — 编辑此挂载点需要批准'

export function FileRowMenu({
  mount,
  sessionId,
  relPath,
  name,
  isDirectory,
  absolutePath,
}: Props): React.ReactElement {
  const addAttachment = useSetAtom(addPendingAttachmentAction)
  const setRenaming = useSetAtom(renamingFilePathAtom)
  const setMoveTarget = useSetAtom(moveTargetAtom)
  const setDeleteTarget = useSetAtom(deleteTargetAtom)

  const isMutable = mount.kind === 'workspace'

  const handleAddToChat = React.useCallback(() => {
    void addAttachment({
      mountId: mount.id,
      relPath,
      name,
      sessionId,
      absolutePath,
    })
  }, [addAttachment, mount.id, relPath, name, sessionId, absolutePath])

  const handleReveal = React.useCallback(async () => {
    try {
      await invoke('reveal_path_in_file_manager', { path: absolutePath })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [absolutePath])

  const handleMove = React.useCallback(() => {
    setMoveTarget({
      mountId: mount.id,
      absolutePath,
      workspaceRelPath: relPath,
      name,
      isDirectory,
    })
  }, [setMoveTarget, mount.id, absolutePath, relPath, name, isDirectory])

  const handleRename = React.useCallback(() => {
    setRenaming(absolutePath)
  }, [setRenaming, absolutePath])

  const handleDelete = React.useCallback(() => {
    setDeleteTarget({
      mountId: mount.id,
      absolutePath,
      workspaceRelPath: relPath,
      name,
      isDirectory,
    })
  }, [setDeleteTarget, mount.id, absolutePath, relPath, name, isDirectory])

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="更多操作"
          title="更多操作"
          onClick={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
          className={cn(
            'size-6 rounded inline-flex items-center justify-center shrink-0',
            'text-muted-foreground hover:text-foreground hover:bg-accent/70',
            'invisible group-hover/row:visible focus-visible:visible data-[state=open]:visible',
            'transition-colors',
          )}
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-44 z-[9999] min-w-0 p-0.5">
        {!isDirectory && (
          <DropdownMenuItem
            className="text-xs py-1 [&>svg]:size-3.5"
            onSelect={handleAddToChat}
          >
            <MessageSquarePlus />
            添加到聊天
          </DropdownMenuItem>
        )}
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          onSelect={() => void handleReveal()}
        >
          <FolderSearch />
          在文件夹中显示
        </DropdownMenuItem>
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleMove()
          }}
        >
          <FolderInput />
          移动到…
        </DropdownMenuItem>
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleRename()
          }}
        >
          <Pencil />
          重命名
        </DropdownMenuItem>
        <DropdownMenuSeparator className="my-0.5" />
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleDelete()
          }}
        >
          <Trash2 />
          删除
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

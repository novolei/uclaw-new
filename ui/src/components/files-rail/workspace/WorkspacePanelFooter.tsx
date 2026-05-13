/**
 * WorkspacePanelFooter — two side-by-side action buttons at the bottom of
 * the workspace files panel: 添加文件 (copies a picked file into the
 * workspace) and 附加文件夹 (registers an external dir as a read-only
 * mount). Disabled when no workspace is active.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { Paperclip, FolderPlus } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  attachWorkspaceDirectory,
  copyFileIntoWorkspace,
  openFileDialog,
  openFolderDialog,
} from '@/lib/tauri-bridge'
import { workspaceAttachedDirsMapAtom } from '@/atoms/agent-atoms'

interface Props {
  workspaceId: string | null
}

export function WorkspacePanelFooter({ workspaceId }: Props): React.ReactElement {
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const [busy, setBusy] = React.useState<'addFile' | 'attachDir' | null>(null)

  const handleAddFile = React.useCallback(async () => {
    if (!workspaceId || busy) return
    setBusy('addFile')
    try {
      const result = await openFileDialog()
      if (!result.files || result.files.length === 0) return
      let added = 0
      for (const f of result.files) {
        try {
          // openFileDialog returns objects with a `path` field on Tauri v2.
          const src = (f && typeof f === 'object' && 'path' in f) ? (f as { path: string }).path : String(f)
          await copyFileIntoWorkspace(workspaceId, src)
          added++
        } catch (err) {
          toast.error('文件复制失败', {
            description: err instanceof Error ? err.message : String(err),
          })
        }
      }
      if (added > 0) {
        toast.success(`已添加 ${added} 个文件到工作区`)
      }
    } finally {
      setBusy(null)
    }
  }, [workspaceId, busy])

  const handleAttachDir = React.useCallback(async () => {
    if (!workspaceId || busy) return
    setBusy('attachDir')
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachWorkspaceDirectory(workspaceId, picked.path)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(workspaceId, updated)
        return m
      })
      toast.success(`已附加目录: ${picked.name}`)
    } catch (err) {
      toast.error('附加目录失败', {
        description: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setBusy(null)
    }
  }, [workspaceId, busy, setWsAttachedMap])

  const disabled = !workspaceId
  const disabledTitle = disabled ? '请先选择工作区' : undefined

  return (
    <footer className="flex-shrink-0 flex items-center gap-2 px-2 py-2 border-t border-border bg-popover">
      <FooterButton
        label="添加文件"
        icon={<Paperclip className="size-4" />}
        onClick={handleAddFile}
        disabled={disabled || busy !== null}
        title={disabledTitle}
      />
      <FooterButton
        label="附加文件夹"
        icon={<FolderPlus className="size-4" />}
        onClick={handleAttachDir}
        disabled={disabled || busy !== null}
        title={disabledTitle}
      />
    </footer>
  )
}

function FooterButton({
  label,
  icon,
  onClick,
  disabled,
  title,
}: {
  label: string
  icon: React.ReactNode
  onClick: () => void
  disabled: boolean
  title?: string
}): React.ReactElement {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        'flex-1 inline-flex items-center justify-center gap-2 h-10 px-3',
        'rounded-md border border-border/60 bg-foreground/[0.02]',
        'text-[12px] text-muted-foreground',
        'transition-colors',
        !disabled && 'hover:bg-foreground/[0.06] hover:border-border hover:text-foreground',
        disabled && 'opacity-40 cursor-not-allowed',
      )}
    >
      {icon}
      <span>{label}</span>
    </button>
  )
}

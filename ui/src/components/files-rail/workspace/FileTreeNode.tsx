import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import { FileRowMenu } from './FileRowMenu'
import { RenameInput } from './RenameInput'
import {
  renamingFilePathAtom,
} from '@/atoms/files-rail-row-atoms'
import { renameArtifact } from '@/lib/tauri-bridge'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import type { MountRoot } from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
  /** Mount this node belongs to — drives menu gating and IPC routing. Optional
   *  for backward-compat with legacy callers (MountSection); when absent, the
   *  row renders without the 3-dot menu. Task 12 tightens this to required. */
  mount?: MountRoot
  /** Active session ID (for addPendingAttachmentAction). Optional, see above. */
  sessionId?: string | null
  /** Sibling names at this depth — used by RenameInput for dup detection. */
  siblings?: Set<string>
  /** Called after a successful rename so the panel can refetch. */
  onRenamed?: (info: { mountId: string; oldRelPath: string; newRelPath: string }) => void
}

export const FileTreeNode = React.memo(function FileTreeNode({
  node,
  depth,
  isExpanded,
  onToggle,
  onFileClick,
  mount,
  sessionId,
  siblings,
  onRenamed,
}: FileTreeNodeProps): React.ReactElement {
  const expanded = isExpanded(node.relPath)
  const isDir = node.kind === 'directory'
  // Legacy MountSection callers don't pass `mount` — skip the menu/rename code
  // path entirely so behaviour matches the pre-Task-9 file.
  const hasMenuContext = mount !== undefined
  const absolutePath = mount ? `${mount.path}/${node.relPath}` : ''
  const renamingPath = useAtomValue(renamingFilePathAtom)
  const setRenaming = useSetAtom(renamingFilePathAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const isRenaming = hasMenuContext && renamingPath === absolutePath

  const handleClick = React.useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      if (isRenaming) return
      if (isDir) void onToggle(node.relPath, true)
      else onFileClick(node, event)
    },
    [isDir, isRenaming, node, onToggle, onFileClick],
  )

  const handleRenameCommit = React.useCallback(async (newName: string) => {
    if (!mount) return
    if (newName === node.name) {
      setRenaming(null)
      return
    }
    const spaceId = spaceIdForMount(mount, currentWorkspaceId)
    if (!spaceId) {
      toast.error('无法解析工作区 ID')
      setRenaming(null)
      return
    }
    const parts = node.relPath.split('/')
    parts[parts.length - 1] = newName
    const newRelPath = parts.join('/')
    try {
      await renameArtifact({
        spaceId,
        oldPath: node.relPath,
        newPath: newRelPath,
      })
      toast.success(`已重命名为 ${newName}`)
      onRenamed?.({ mountId: mount.id, oldRelPath: node.relPath, newRelPath })
      setRenaming(null)
    } catch (err) {
      toast.error('重命名失败', {
        description: err instanceof Error ? err.message : String(err),
      })
      // leave rename open so user can retry
    }
  }, [node.name, node.relPath, mount, currentWorkspaceId, onRenamed, setRenaming])

  const handleRenameCancel = React.useCallback(() => {
    setRenaming(null)
  }, [setRenaming])

  const indent = depth * 12

  return (
    <>
      <div
        className={cn(
          'group/row relative flex items-center w-full h-[22px] px-2 gap-1',
          'text-[12px] text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
        )}
        style={{ paddingLeft: 8 + indent }}
      >
        <button
          type="button"
          onClick={handleClick}
          className={cn(
            'flex-1 min-w-0 flex items-center gap-1 text-left h-full',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          )}
          title={node.relPath}
        >
          {isDir ? (
            expanded ? (
              <ChevronDown size={12} className="shrink-0 text-foreground/40" />
            ) : (
              <ChevronRight size={12} className="shrink-0 text-foreground/40" />
            )
          ) : (
            <span className="w-3 shrink-0" aria-hidden />
          )}
          <FileTypeIcon
            name={node.name}
            isDirectory={isDir}
            isOpen={isDir && expanded}
            size={14}
            className="shrink-0"
          />
          {isRenaming && mount && siblings ? (
            <RenameInput
              initialName={node.name}
              siblings={siblings}
              onCommit={(newName) => void handleRenameCommit(newName)}
              onCancel={handleRenameCancel}
            />
          ) : (
            <span className="truncate font-mono tabular-nums">{node.name}</span>
          )}
        </button>
        {hasMenuContext && mount && !isRenaming && (
          <FileRowMenu
            mount={mount}
            sessionId={sessionId ?? null}
            relPath={node.relPath}
            name={node.name}
            isDirectory={isDir}
            absolutePath={absolutePath}
          />
        )}
      </div>
      {isDir && expanded && node.children && (
        <>
          {(() => {
            const childSiblings = new Set(node.children.map((c) => c.name))
            return node.children.map((child) => (
              <FileTreeNode
                key={child.relPath}
                node={child}
                depth={depth + 1}
                isExpanded={isExpanded}
                onToggle={onToggle}
                onFileClick={onFileClick}
                mount={mount}
                sessionId={sessionId}
                siblings={childSiblings}
                onRenamed={onRenamed}
              />
            ))
          })()}
        </>
      )}
    </>
  )
})

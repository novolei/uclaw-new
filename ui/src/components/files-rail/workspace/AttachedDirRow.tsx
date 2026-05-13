/**
 * AttachedDirRow — one collapsible row per attached-dir mount.
 *
 * - Chevron + folder icon + label + Lock badge (when !editable).
 * - 3-dot menu on the row shows only 在文件夹中显示 (mount-label rename
 *   is out of scope per spec § Section 2).
 * - When expanded, mounts the watcher (filesRailWatchStart) and
 *   recursively renders FileTreeNode children. Collapsing unregisters
 *   the watcher.
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { ChevronRight, Lock, MoreHorizontal, FolderSearch, FolderMinus } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import { FileTreeNode } from './FileTreeNode'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { useFilesRailWatcher } from '@/components/files-rail/hooks/useFilesRailWatcher'
import {
  filesRailWatchStart,
  filesRailWatchStop,
  detachWorkspaceDirectory,
  detachSessionDirectory,
} from '@/lib/tauri-bridge'
import {
  expandedPathsAtomFamily,
  type MountRoot,
} from '@/atoms/files-rail-atoms'
import {
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface Props {
  mount: MountRoot
  sessionId: string | null
  onFileClick: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

const TOP_EXPAND_KEY = '__top__'  // sentinel for the row's own collapse state

// Mount IDs encode the scope: "workspace-attached:<sid>:<hash>" vs
// "attached:<sid>:<hash>". We pick the right detach IPC by prefix.
function isWorkspaceAttached(mountId: string): boolean {
  return mountId.startsWith('workspace-attached:')
}

export function AttachedDirRow({ mount, sessionId, onFileClick }: Props): React.ReactElement {
  const [expanded, setExpanded] = useAtom(expandedPathsAtomFamily(mount.id))
  const isExpanded = expanded.has(TOP_EXPAND_KEY)
  const treeApi = useFileTree(mount.id, sessionId)
  useFilesRailWatcher(mount.id, treeApi.applyExternalChanges)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const [detaching, setDetaching] = React.useState(false)

  // Mount the OS watcher only while expanded.
  React.useEffect(() => {
    if (!isExpanded) return
    void filesRailWatchStart(mount.id, sessionId).catch(() => { /* silent */ })
    return () => {
      void filesRailWatchStop(mount.id).catch(() => { /* idempotent */ })
    }
  }, [isExpanded, mount.id, sessionId])

  const toggleTop = React.useCallback(() => {
    const next = new Set(expanded)
    if (next.has(TOP_EXPAND_KEY)) next.delete(TOP_EXPAND_KEY)
    else next.add(TOP_EXPAND_KEY)
    setExpanded(next)
  }, [expanded, setExpanded])

  const handleReveal = React.useCallback(async () => {
    try {
      await invoke('reveal_path_in_file_manager', { path: mount.path })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [mount.path])

  const handleDetach = React.useCallback(async () => {
    if (detaching) return
    const isWsAttach = isWorkspaceAttached(mount.id)
    if (isWsAttach && !currentWorkspaceId) {
      toast.error('无法解析当前工作区')
      return
    }
    setDetaching(true)
    try {
      if (isWsAttach && currentWorkspaceId) {
        const updated = await detachWorkspaceDirectory(currentWorkspaceId, mount.path)
        setWsAttachedMap((prev) => {
          const m = new Map(prev)
          m.set(currentWorkspaceId, updated)
          return m
        })
      } else if (sessionId) {
        const updated = await detachSessionDirectory(sessionId, mount.path)
        setSessionAttachedMap((prev) => {
          const m = new Map(prev)
          m.set(sessionId, updated)
          return m
        })
      } else {
        toast.error('无法识别附加目录所属的会话')
        return
      }
      toast.success(`已移除附加: ${mount.label}`)
    } catch (err) {
      toast.error('移除附加失败', {
        description: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setDetaching(false)
    }
  }, [detaching, mount.id, mount.path, mount.label, currentWorkspaceId, sessionId, setWsAttachedMap, setSessionAttachedMap])

  const isChildExpanded = React.useCallback(
    (rel: string) => expanded.has(rel),
    [expanded],
  )

  const handleChildToggle = React.useCallback(
    async (rel: string, isDir: boolean) => {
      // Delegate to the useFileTree's toggleExpand which handles lazy load
      await treeApi.toggleExpand(rel, isDir)
    },
    [treeApi],
  )

  const handleChildFileClick = React.useCallback(
    (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => onFileClick(mount, node, event),
    [mount, onFileClick],
  )

  const topSiblings = React.useMemo(
    () => new Set(treeApi.nodes.map((n) => n.name)),
    [treeApi.nodes],
  )

  return (
    <section>
      <div className="group/row relative flex items-center w-full h-[24px] px-2 gap-1 text-[12px] text-foreground/90 hover:bg-foreground/[0.04] transition-colors">
        <button
          type="button"
          onClick={toggleTop}
          className="flex-1 min-w-0 flex items-center gap-1 text-left h-full focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          title={mount.path}
        >
          <ChevronRight
            size={12}
            className={cn(
              'shrink-0 text-foreground/50 transition-transform duration-150',
              isExpanded && 'rotate-90',
            )}
          />
          <FileTypeIcon name={mount.label} isDirectory size={14} className="shrink-0" />
          <span className="truncate font-medium">{mount.label}</span>
          {!mount.editable && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Lock className="size-2.5 text-muted-foreground/60 shrink-0" aria-label="只读" />
              </TooltipTrigger>
              <TooltipContent side="bottom">
                <p className="text-[11px]">只读 — 编辑此挂载点需要批准</p>
              </TooltipContent>
            </Tooltip>
          )}
        </button>
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
            <DropdownMenuItem
              className="text-xs py-1 [&>svg]:size-3.5"
              onSelect={() => void handleReveal()}
            >
              <FolderSearch />
              在文件夹中显示
            </DropdownMenuItem>
            <DropdownMenuSeparator className="my-0.5" />
            <DropdownMenuItem
              className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
              disabled={detaching}
              onSelect={(e) => {
                e.preventDefault()
                void handleDetach()
              }}
            >
              <FolderMinus />
              {detaching ? '移除中…' : '移除附加'}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {isExpanded && treeApi.loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive truncate">
          {treeApi.errorMessage ?? '加载失败'}
        </div>
      )}
      {isExpanded && treeApi.loadState === 'ready' && treeApi.nodes.length === 0 && (
        <div className="px-3 py-2 text-[11px] text-muted-foreground/70">空文件夹</div>
      )}
      {isExpanded && treeApi.nodes.length > 0 && (
        <div className="min-h-0">
          {treeApi.nodes.map((child) => (
            <FileTreeNode
              key={child.relPath}
              node={child}
              depth={1}
              isExpanded={isChildExpanded}
              onToggle={handleChildToggle}
              onFileClick={handleChildFileClick}
              mount={mount}
              sessionId={sessionId}
              siblings={topSiblings}
            />
          ))}
        </div>
      )}
    </section>
  )
}

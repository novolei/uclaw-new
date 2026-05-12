import * as React from 'react'
import { FolderOpen, RefreshCw, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { FileTreeNode } from './FileTreeNode'
import { filesRailWatchStart, filesRailWatchStop } from '@/lib/tauri-bridge'
import type { MountRoot } from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface MountSectionProps {
  mount: MountRoot
  onFileClick: (mount: MountRoot, node: TreeNode) => void
}

export function MountSection({ mount, onFileClick }: MountSectionProps): React.ReactElement {
  const { nodes, loadState, errorMessage, isExpanded, toggleExpand, reload } = useFileTree(mount.id)

  React.useEffect(() => {
    void filesRailWatchStart(mount.id).catch(() => {
      /* silent — events just won't arrive */
    })
    return () => {
      void filesRailWatchStop(mount.id).catch(() => {
        /* idempotent on backend */
      })
    }
  }, [mount.id])

  const handleFileClick = React.useCallback(
    (node: TreeNode) => onFileClick(mount, node),
    [mount, onFileClick],
  )

  return (
    <section className="flex flex-col mb-3">
      <header className="flex items-center gap-1 px-2 h-[28px] flex-shrink-0">
        <FolderOpen className="size-3 text-muted-foreground" />
        <span className="text-[11px] font-medium text-muted-foreground truncate">{mount.label}</span>
        <span className="ml-auto" />
        <button
          type="button"
          onClick={() => void reload()}
          aria-label="刷新"
          className="size-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
        >
          <RefreshCw className={cn('size-2.5', loadState === 'loading' && 'animate-spin')} />
        </button>
      </header>
      {loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive flex items-center gap-1">
          <AlertTriangle size={12} aria-hidden />
          <span className="truncate">{errorMessage ?? '加载失败'}</span>
        </div>
      )}
      {loadState === 'ready' && nodes.length === 0 && (
        <div className="px-3 py-2 text-[11px] text-muted-foreground">这里还没有文件</div>
      )}
      <div className="min-h-0">
        {nodes.map((node) => (
          <FileTreeNode
            key={node.relPath}
            node={node}
            depth={0}
            isExpanded={isExpanded}
            onToggle={toggleExpand}
            onFileClick={handleFileClick}
          />
        ))}
      </div>
    </section>
  )
}

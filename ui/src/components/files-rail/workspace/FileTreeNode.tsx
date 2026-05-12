import * as React from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode) => void
}

export const FileTreeNode = React.memo(function FileTreeNode({
  node,
  depth,
  isExpanded,
  onToggle,
  onFileClick,
}: FileTreeNodeProps): React.ReactElement {
  const expanded = isExpanded(node.relPath)
  const isDir = node.kind === 'directory'

  const handleClick = React.useCallback(() => {
    if (isDir) void onToggle(node.relPath, true)
    else onFileClick(node)
  }, [isDir, node, onToggle, onFileClick])

  const indent = depth * 12

  return (
    <>
      <button
        type="button"
        onClick={handleClick}
        className={cn(
          'flex items-center w-full h-[22px] px-2 gap-1 text-[12px] text-left',
          'text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        )}
        style={{ paddingLeft: 8 + indent }}
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
        <span className="truncate font-mono tabular-nums">{node.name}</span>
      </button>
      {isDir && expanded && node.children && (
        <>
          {node.children.map((child) => (
            <FileTreeNode
              key={child.relPath}
              node={child}
              depth={depth + 1}
              isExpanded={isExpanded}
              onToggle={onToggle}
              onFileClick={onFileClick}
            />
          ))}
        </>
      )}
    </>
  )
})

import * as React from 'react'
import { useAtom } from 'jotai'
import { mountRootsAtomFamily, type MountRoot } from '@/atoms/files-rail-atoms'
import { filesRailListMounts } from '@/lib/tauri-bridge'
import { MountSection } from './MountSection'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

export function WorkspaceFilesPanel({
  sessionId,
  onFileClick,
}: WorkspaceFilesPanelProps): React.ReactElement {
  const [mounts, setMounts] = useAtom(mountRootsAtomFamily(sessionId))

  React.useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const fetched = await filesRailListMounts(sessionId)
        if (!cancelled) setMounts(fetched)
      } catch {
        if (!cancelled) setMounts([])
      }
    })()
    return () => {
      cancelled = true
    }
  }, [sessionId, setMounts])

  const handleClick = React.useCallback(
    (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => {
      onFileClick?.(mount, node, event)
    },
    [onFileClick],
  )

  if (mounts.length === 0) {
    return (
      <div className="p-4 text-[12px] text-muted-foreground">
        还没有挂载点 — 点击右上的 + 按钮添加目录
      </div>
    )
  }

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-y-auto py-2">
      {mounts.map((m) => (
        <MountSection key={m.id} mount={m} sessionId={sessionId} onFileClick={handleClick} />
      ))}
    </div>
  )
}

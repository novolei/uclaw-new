import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { mountRootsAtomFamily, type MountRoot } from '@/atoms/files-rail-atoms'
import {
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
} from '@/atoms/agent-atoms'
import { filesRailListMounts } from '@/lib/tauri-bridge'
import { MountSection } from './MountSection'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

/**
 * Fingerprint the two attached-dir maps so we can use them as a stable
 * useEffect dep. We don't know the active workspace id from this
 * component's props, so we hash every entry — cheap (≤ a few KB of
 * strings in practice) and means any attach/detach anywhere triggers
 * a refetch. The mount-list backend ignores sessions whose workspace
 * isn't active, so the refetch is idempotent for unrelated changes.
 */
function fingerprintAttachedDirs(
  wsMap: Map<string, string[]>,
  sessionMap: Map<string, string[]>,
): string {
  const wsEntries = Array.from(wsMap.entries())
    .map(([k, v]) => `${k}:${v.join(',')}`)
    .sort()
  const sessionEntries = Array.from(sessionMap.entries())
    .map(([k, v]) => `${k}:${v.join(',')}`)
    .sort()
  return `${wsEntries.join('|')}#${sessionEntries.join('|')}`
}

export function WorkspaceFilesPanel({
  sessionId,
  onFileClick,
}: WorkspaceFilesPanelProps): React.ReactElement {
  const [mounts, setMounts] = useAtom(mountRootsAtomFamily(sessionId))
  // Watch both attached-dir maps so the rail refetches its mount list
  // when the user attaches/detaches a directory from anywhere (SidePanel
  // "+ 添加目录" button, AppShell drop). Without this dep, the user has
  // to switch spaces before a freshly-attached dir shows up.
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const attachedFingerprint = fingerprintAttachedDirs(wsAttachedMap, sessionAttachedMap)

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
  }, [sessionId, attachedFingerprint, setMounts])

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

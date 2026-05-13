/**
 * WorkspaceFilesPanel — the Files tab's content for the workspace tab.
 *
 * Composition (top to bottom):
 *   - WorkspacePanelHeader
 *   - AttachedDirsSection  (subtitle + AttachedDirRow × N, only when any)
 *   - WorkspaceFilesSection (subtitle + flat FileTreeNode tree)
 *   - WorkspacePanelFooter (添加文件 / 附加文件夹)
 *
 * The three single-target dialogs (MoveToDialog / DeleteConfirmDialog +
 * the inline RenameInput inside FileTreeNode) are mounted here at panel
 * scope so atom transitions surface their UI regardless of which row
 * triggered them.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { mountRootsAtomFamily, type MountRoot } from '@/atoms/files-rail-atoms'
import {
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'
import { filesRailListMounts } from '@/lib/tauri-bridge'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { useFilesRailWatcher } from '@/components/files-rail/hooks/useFilesRailWatcher'
import { filesRailWatchStart, filesRailWatchStop } from '@/lib/tauri-bridge'
import { FileTreeNode } from './FileTreeNode'
import { AttachedDirRow } from './AttachedDirRow'
import { WorkspacePanelHeader } from './WorkspacePanelHeader'
import { WorkspacePanelFooter } from './WorkspacePanelFooter'
import { MoveToDialog } from './MoveToDialog'
import { DeleteConfirmDialog } from './DeleteConfirmDialog'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

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
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const attachedFingerprint = fingerprintAttachedDirs(wsAttachedMap, sessionAttachedMap)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)

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
    return () => { cancelled = true }
  }, [sessionId, attachedFingerprint, setMounts])

  const workspaceMount = mounts.find((m) => m.kind === 'workspace') ?? null
  const attachedMounts = mounts.filter((m) => m.kind === 'attached_dir')
  const workspaceRootPath = workspaceMount?.path ?? null

  return (
    <div className="flex flex-col h-full min-h-0">
      <WorkspacePanelHeader
        sessionId={sessionId}
        workspaceRootPath={workspaceRootPath}
      />
      <div className="flex-1 min-h-0 overflow-y-auto py-1">
        {workspaceMount && (
          <WorkspaceFilesBody
            mount={workspaceMount}
            sessionId={sessionId}
            showSubtitle={attachedMounts.length > 0}
            onFileClick={onFileClick}
          />
        )}
        {attachedMounts.length > 0 && (
          <section className="mt-2">
            <div className="text-[11px] font-medium text-muted-foreground/80 px-3 pt-2 pb-1">
              附加目录（Agent 可以读取并操作此外部文件夹）
            </div>
            {attachedMounts.map((m) => (
              <AttachedDirRow
                key={m.id}
                mount={m}
                sessionId={sessionId}
                onFileClick={(mt, n, e) => onFileClick?.(mt, n, e)}
              />
            ))}
          </section>
        )}
      </div>
      <WorkspacePanelFooter workspaceId={currentWorkspaceId ?? null} />
      <MoveToDialog
        workspaceRootPath={workspaceRootPath}
        mountKindForTarget="workspace"
      />
      <DeleteConfirmDialog mountKindForTarget="workspace" />
    </div>
  )
}

function WorkspaceFilesBody({
  mount,
  sessionId,
  showSubtitle,
  onFileClick,
}: {
  mount: MountRoot
  sessionId: string | null
  showSubtitle: boolean
  onFileClick?: (m: MountRoot, n: TreeNode, e: React.MouseEvent<HTMLButtonElement>) => void
}): React.ReactElement {
  const treeApi = useFileTree(mount.id, sessionId)
  useFilesRailWatcher(mount.id, treeApi.applyExternalChanges)

  React.useEffect(() => {
    void filesRailWatchStart(mount.id, sessionId).catch(() => { /* silent */ })
    return () => {
      void filesRailWatchStop(mount.id).catch(() => { /* idempotent */ })
    }
  }, [mount.id, sessionId])

  const siblings = React.useMemo(
    () => new Set(treeApi.nodes.map((n) => n.name)),
    [treeApi.nodes],
  )

  const handleFileClick = React.useCallback(
    (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) =>
      onFileClick?.(mount, node, event),
    [mount, onFileClick],
  )

  return (
    <section>
      {showSubtitle && (
        <div className="text-[11px] font-medium text-muted-foreground/80 px-3 pt-2 pb-1">
          工作文件（存储于该工作区目录）
        </div>
      )}
      {treeApi.loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive truncate">
          {treeApi.errorMessage ?? '加载失败'}
        </div>
      )}
      {treeApi.loadState === 'ready' && treeApi.nodes.length === 0 && !showSubtitle && (
        <div className="px-3 py-3 text-[12px] text-muted-foreground">
          工作区还没有文件 — 用下方的「添加文件」或「附加文件夹」开始
        </div>
      )}
      {treeApi.nodes.length > 0 && (
        <div className="min-h-0">
          {treeApi.nodes.map((node) => (
            <FileTreeNode
              key={node.relPath}
              node={node}
              depth={0}
              isExpanded={treeApi.isExpanded}
              onToggle={treeApi.toggleExpand}
              onFileClick={handleFileClick}
              mount={mount}
              sessionId={sessionId}
              siblings={siblings}
            />
          ))}
        </div>
      )}
    </section>
  )
}

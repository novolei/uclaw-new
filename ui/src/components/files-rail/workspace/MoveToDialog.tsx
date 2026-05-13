/**
 * MoveToDialog — opens the OS folder picker, validates the choice falls
 * inside the workspace dir, then calls moveArtifact.
 *
 * No modal UI of our own — we delegate to @tauri-apps/plugin-dialog.
 * This component is rendered once at the rail level; it reacts to
 * moveTargetAtom transitioning non-null by opening the picker.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { toast } from 'sonner'
import { moveArtifact, openFolderDialog } from '@/lib/tauri-bridge'
import { moveTargetAtom } from '@/atoms/files-rail-row-atoms'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import type { MountKind } from '@/atoms/files-rail-atoms'

interface Props {
  /** Workspace root absolute path (used to validate the picked dir). */
  workspaceRootPath: string | null
  /** mountKind for the target — used by spaceIdForMount. Defaults to 'workspace'. */
  mountKindForTarget?: MountKind
  /** Called after a successful move so the caller can refetch source + dest parents. */
  onMoved?: (info: { mountId: string; srcAbsolutePath: string; destAbsolutePath: string }) => void
}

export function MoveToDialog({
  workspaceRootPath,
  mountKindForTarget = 'workspace',
  onMoved,
}: Props): null {
  const [target, setTarget] = useAtom(moveTargetAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const runningRef = React.useRef(false)

  React.useEffect(() => {
    if (!target || runningRef.current) return
    runningRef.current = true

    void (async () => {
      try {
        const picked = await openFolderDialog()
        if (!picked) {
          return
        }
        if (!workspaceRootPath) {
          toast.error('工作区路径未解析，无法移动')
          return
        }
        const wsRoot = workspaceRootPath.replace(/\/+$/, '')
        if (picked.path !== wsRoot && !picked.path.startsWith(wsRoot + '/')) {
          toast.error('只能移动到当前工作区内的文件夹')
          return
        }
        const spaceId = spaceIdForMount({ id: target.mountId, kind: mountKindForTarget }, currentWorkspaceId)
        if (!spaceId) {
          toast.error('无法解析工作区 ID')
          return
        }
        // destPath is workspace-relative, joined with the original basename.
        const destRelDir = picked.path === wsRoot ? '' : picked.path.slice(wsRoot.length + 1)
        const destRelPath = destRelDir.length > 0 ? `${destRelDir}/${target.name}` : target.name
        if (destRelPath === target.workspaceRelPath) {
          // No-op — picked the existing parent.
          return
        }
        try {
          await moveArtifact({
            spaceId,
            srcPath: target.workspaceRelPath,
            destPath: destRelPath,
          })
          toast.success(`已移动 ${target.name}`)
          const destAbsolute = `${wsRoot}/${destRelPath}`
          onMoved?.({
            mountId: target.mountId,
            srcAbsolutePath: target.absolutePath,
            destAbsolutePath: destAbsolute,
          })
        } catch (err) {
          toast.error('移动失败', {
            description: err instanceof Error ? err.message : String(err),
          })
        }
      } finally {
        runningRef.current = false
        setTarget(null)
      }
    })()
  }, [target, workspaceRootPath, currentWorkspaceId, mountKindForTarget, onMoved, setTarget])

  return null
}

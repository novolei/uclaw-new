/**
 * DeleteConfirmDialog — shadcn AlertDialog wrapping deleteArtifactRecursive.
 *
 * Driven by deleteTargetAtom: when non-null the dialog is open. Confirm
 * fires the IPC; on success clears the atom + toasts; on error keeps
 * the dialog open with the error message.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { deleteArtifactRecursive } from '@/lib/tauri-bridge'
import { deleteTargetAtom } from '@/atoms/files-rail-row-atoms'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import type { MountKind } from '@/atoms/files-rail-atoms'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import { cn } from '@/lib/utils'

interface Props {
  /** mountKind for the current target — used by spaceIdForMount. */
  mountKindForTarget?: MountKind
  /** Called after a successful delete so the panel can refetch the parent. */
  onDeleted?: (target: { mountId: string; absolutePath: string }) => void
}

export function DeleteConfirmDialog({ mountKindForTarget = 'workspace', onDeleted }: Props): React.ReactElement {
  const [target, setTarget] = useAtom(deleteTargetAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const [submitting, setSubmitting] = React.useState(false)
  const [submitError, setSubmitError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!target) setSubmitError(null)
  }, [target])

  const handleCancel = React.useCallback(() => {
    if (submitting) return
    setTarget(null)
  }, [submitting, setTarget])

  const handleConfirm = React.useCallback(async () => {
    if (!target) return
    const spaceId = spaceIdForMount({ id: target.mountId, kind: mountKindForTarget }, currentWorkspaceId)
    if (!spaceId) {
      setSubmitError('无法解析工作区 ID')
      return
    }
    setSubmitting(true)
    setSubmitError(null)
    try {
      await deleteArtifactRecursive(spaceId, target.workspaceRelPath)
      toast.success(`已删除 ${target.name}`)
      onDeleted?.({ mountId: target.mountId, absolutePath: target.absolutePath })
      setTarget(null)
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err))
    } finally {
      setSubmitting(false)
    }
  }, [target, currentWorkspaceId, mountKindForTarget, onDeleted, setTarget])

  const open = target !== null

  return (
    <AlertDialog open={open} onOpenChange={(o) => { if (!o) handleCancel() }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>确认删除</AlertDialogTitle>
          <AlertDialogDescription>
            {target && (
              <>
                确定要删除 <strong className="text-foreground">{target.name}</strong> 吗？此操作不可撤销。
                {target.isDirectory && <span className="text-muted-foreground">（包含其下全部内容）</span>}
              </>
            )}
            {submitError && (
              <span className="block mt-2 text-destructive text-[11px]">{submitError}</span>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={submitting} onClick={handleCancel}>取消</AlertDialogCancel>
          <AlertDialogAction
            disabled={submitting}
            onClick={(e) => { e.preventDefault(); void handleConfirm() }}
            className={cn('bg-destructive text-destructive-foreground hover:bg-destructive/90')}
          >
            {submitting ? '删除中…' : '删除'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

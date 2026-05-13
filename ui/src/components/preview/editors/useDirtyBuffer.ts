/**
 * useDirtyBuffer — universal dirty tracking + close intercepts.
 *
 * Engaged for BOTH save modes (explicit code-save and auto markdown-save).
 * The dirty atom is the single source of truth for:
 *   - usePreviewRefresh's "don't bump refetch for dirty files" guard
 *   - openPreviewAction / closePreviewAction's confirm-on-close prompts
 *   - the beforeunload event guard below
 *
 * Auto-save mode behaviour: while a save is in flight, `currentContent`
 * still differs from `baselineContent` (parent only promotes baseline
 * after the save succeeds). We stay marked dirty across the save, which
 * is exactly what we want — the dirty-guard then blocks any inbound
 * watcher/refresh bump from clobbering the in-flight edit.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  dirtyBuffersAtom,
  setDirtyBufferAction,
  clearDirtyBufferAction,
} from '@/atoms/preview-editor-atoms'

export interface UseDirtyBufferArgs {
  filePath: string
  baselineContent: string
  baselineMtimeMs: number
  currentContent: string
}

export interface UseDirtyBufferResult {
  isDirty: boolean
  /** Manually clear (called after a successful save). */
  clear: () => void
}

export function useDirtyBuffer(args: UseDirtyBufferArgs): UseDirtyBufferResult {
  const { filePath, baselineContent, baselineMtimeMs, currentContent } = args
  const buffers = useAtomValue(dirtyBuffersAtom)
  const setDirty = useSetAtom(setDirtyBufferAction)
  const clearDirty = useSetAtom(clearDirtyBufferAction)

  const isDirty = currentContent !== baselineContent

  // Register or update the dirty entry whenever currentContent diverges;
  // clear when content returns to baseline (typically after a save).
  React.useEffect(() => {
    if (isDirty) {
      setDirty({ filePath, content: currentContent, baselineMtimeMs })
    } else if (buffers.has(filePath)) {
      clearDirty(filePath)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath, currentContent, isDirty])

  // beforeunload guard — only when buffer is dirty.
  React.useEffect(() => {
    if (!isDirty) return
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      // Chrome ignores custom strings; setting returnValue is the
      // protocol-correct way to opt into the native confirm.
      e.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [isDirty])

  return {
    isDirty,
    clear: React.useCallback(() => clearDirty(filePath), [clearDirty, filePath]),
  }
}

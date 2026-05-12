/**
 * useDirtyBuffer — code-mode dirty tracking + close intercepts.
 *
 * Only engaged for `saveMode === 'explicit'` (code/text formats).
 * Markdown editors use auto-save and don't register here.
 *
 * Responsibilities:
 *   - Register a DirtyBuffer when content first diverges from baseline
 *   - Clear the buffer on successful save (i.e. when content returns
 *     to baseline OR the editor calls .clear() explicitly)
 *   - beforeunload event guard while dirty (browser/Tauri window close)
 *
 * File-switch + panel-close intercepts live in preview-panel-atoms.ts —
 * openPreviewAction and closePreviewAction read dirtyBuffersAtom and
 * surface a window.confirm before transitioning.
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
  saveMode: 'explicit' | 'auto'
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
  const { filePath, saveMode, baselineContent, baselineMtimeMs, currentContent } = args
  const buffers = useAtomValue(dirtyBuffersAtom)
  const setDirty = useSetAtom(setDirtyBufferAction)
  const clearDirty = useSetAtom(clearDirtyBufferAction)

  const isDirty = saveMode === 'explicit' && currentContent !== baselineContent

  // Register or update the dirty entry whenever currentContent changes
  // while dirty; clear when content returns to baseline.
  React.useEffect(() => {
    if (saveMode !== 'explicit') return
    if (isDirty) {
      setDirty({ filePath, content: currentContent, baselineMtimeMs })
    } else if (buffers.has(filePath)) {
      clearDirty(filePath)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [saveMode, filePath, currentContent, isDirty])

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

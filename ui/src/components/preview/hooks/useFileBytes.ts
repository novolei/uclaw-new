/**
 * useFileBytes — Fetch bytes for a given (mountId, relPath, sessionId).
 *
 * Returns:
 *   - status: 'idle' | 'loading' | 'ready' | 'error'
 *   - bytes / size / truncated when ready
 *   - error message when error
 *
 * Re-fetches when the W1 refresh atom for the resolvedPath bumps (agent file
 * writes, window focus, manual refresh). Stale fetches are guarded with a
 * cancellation flag.
 */

import * as React from 'react'
import { usePreviewRefresh } from '@/hooks/usePreviewRefresh'
import { previewReadBytes, type PreviewBytes } from '@/lib/tauri-bridge'
import type { PreviewFileTarget } from '@/atoms/preview-panel-atoms'

export type FileBytesState =
  | { status: 'idle' }
  | { status: 'loading' }
  | {
      status: 'ready'
      bytes: Uint8Array
      size: number
      truncated: boolean
      mtimeMs: number
      resolvedPath: string
    }
  | { status: 'error'; message: string }

export function useFileBytes(target: PreviewFileTarget | null): FileBytesState {
  const [state, setState] = React.useState<FileBytesState>({ status: 'idle' })
  // resolvedPath is what useFileBytes returns; we want refresh keyed on it
  // when known. Fall back to mountId:relPath for the pre-fetch period.
  const refreshKey = target ? (target.absolutePath ?? `${target.mountId}:${target.relPath}`) : ''
  const refreshVersion = usePreviewRefresh(refreshKey || null)

  React.useEffect(() => {
    if (!target) {
      setState({ status: 'idle' })
      return
    }
    let cancelled = false
    setState({ status: 'loading' })
    void (async () => {
      try {
        const result: PreviewBytes = await previewReadBytes(
          target.mountId,
          target.relPath,
          target.sessionId ?? null,
        )
        if (cancelled) return
        setState({
          status: 'ready',
          bytes: result.bytes,
          size: result.size,
          truncated: result.truncated,
          mtimeMs: result.mtimeMs,
          resolvedPath: result.resolvedPath,
        })
      } catch (err) {
        if (cancelled) return
        setState({ status: 'error', message: String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [target?.mountId, target?.relPath, target?.sessionId, refreshVersion])

  return state
}

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

function targetKeyOf(target: PreviewFileTarget | null): string {
  if (!target) return ''
  return `${target.mountId}|${target.relPath}|${target.sessionId ?? ''}`
}

export function useFileBytes(target: PreviewFileTarget | null): FileBytesState {
  const [state, setState] = React.useState<FileBytesState>({ status: 'idle' })
  // Tracks which target's bytes are currently stored in `state`. Survives
  // re-renders so we can detect — DURING render, before the new effect
  // runs — that the state belongs to a previous target and must not be
  // shown. Without this gate, the render that immediately follows a
  // target switch reads the previous file's bytes (the effect that
  // re-fetches hasn't run yet) and PreviewSurface mounts EditorSurface
  // with stale `initialContent`. Even though a later render would mount
  // a fresh editor, EditorSurface's useState(initialContent) captured
  // the stale value at mount time, so the visible content stays stale.
  const loadedKeyRef = React.useRef<string>('')
  const targetKey = targetKeyOf(target)

  const refreshKey = target ? (target.absolutePath ?? `${target.mountId}:${target.relPath}`) : ''
  const refreshVersion = usePreviewRefresh(refreshKey || null)

  React.useEffect(() => {
    if (!target) {
      loadedKeyRef.current = ''
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
        loadedKeyRef.current = targetKey
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
        loadedKeyRef.current = targetKey
        setState({ status: 'error', message: String(err) })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [targetKey, refreshVersion])

  // Gate: report 'loading' whenever the stored state was produced for a
  // different target than the one the caller is asking about now. This
  // closes the race between target prop change and the effect re-run.
  if (
    target !== null &&
    (state.status === 'ready' || state.status === 'error') &&
    loadedKeyRef.current !== targetKey
  ) {
    return { status: 'loading' }
  }
  return state
}

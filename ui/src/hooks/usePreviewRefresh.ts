/**
 * usePreviewRefresh — Wave 1 of the renderer quick-wins port.
 *
 * Returns the current refresh version for a file path and subscribes to the
 * triggers that should bump it: window focus and agent-side file writes.
 * Consumer modules (W4 useFileBytes, codeHighlightCache key) include the
 * returned number so a bump naturally invalidates their state.
 *
 * Dirty-guard (2026-05-13, ported from if2Ai): when this file has an
 * unsaved local edit (i.e. it's in `dirtyBuffersAtom`), inbound bumps are
 * SUPPRESSED. The user's in-progress draft must never be silently replaced
 * by a fresh read of disk content — that's the whole reason if2Ai's preview
 * panel never gets "file changed on disk" race surprises.
 *
 * Triggers:
 *   - tauri://focus (window regained focus — refresh clean buffers)
 *   - agent:file-written (agent's file-write tool completed)
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import {
  previewRefreshVersionAtomFamily,
  bumpPreviewRefreshAtom,
} from '@/atoms/preview-atoms'
import { isDirtyAtomFamily } from '@/atoms/preview-editor-atoms'

interface FileWrittenPayload {
  path: string
}

export function usePreviewRefresh(filePath: string | null): number {
  // Empty-string key is a deliberate sentinel for the null case. The effect
  // below short-circuits when filePath is null, so this atom is never bumped
  // by this hook — but reading it keeps the hook signature stable.
  const version = useAtomValue(
    previewRefreshVersionAtomFamily(filePath ?? ''),
  )
  const bump = useSetAtom(bumpPreviewRefreshAtom)
  const isDirty = useAtomValue(isDirtyAtomFamily(filePath ?? ''))

  // Capture dirty state in a ref so the listener callbacks (registered
  // once per filePath) always see the latest value without re-subscribing.
  const isDirtyRef = React.useRef(isDirty)
  React.useEffect(() => {
    isDirtyRef.current = isDirty
  }, [isDirty])

  React.useEffect(() => {
    if (!filePath) return
    let unlistenFocus: (() => void) | undefined
    let unlistenWrite: (() => void) | undefined
    let cancelled = false

    void (async () => {
      const u1 = await listen('tauri://focus', () => {
        if (cancelled) return
        // Skip refresh when there's an unsaved draft — the draft wins.
        if (isDirtyRef.current) return
        bump(filePath)
      })
      const u2 = await listen<FileWrittenPayload>('agent:file-written', (evt) => {
        if (cancelled) return
        if (evt.payload?.path !== filePath) return
        // Agent just wrote this file. If the user has an unsaved local edit,
        // preserve their draft (matches if2Ai's behaviour — explicit conflict
        // resolution is surfaced on-demand, not silently clobbered).
        if (isDirtyRef.current) return
        bump(filePath)
      })
      unlistenFocus = u1
      unlistenWrite = u2
    })()

    return () => {
      cancelled = true
      unlistenFocus?.()
      unlistenWrite?.()
    }
  }, [filePath, bump])

  return version
}

/**
 * usePreviewRefresh — Wave 1 of the renderer quick-wins port.
 *
 * Returns the current refresh version for a file path and subscribes to the
 * triggers that should bump it: window focus and agent-side file writes.
 * Consumer modules (W4 useFileBytes, codeHighlightCache key) include the
 * returned number so a bump naturally invalidates their state.
 *
 * Triggers added in later waves:
 *   - W3: files_rail:change
 *   - W4: manual refresh button via bumpPreviewRefreshAtom
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import {
  previewRefreshVersionAtomFamily,
  bumpPreviewRefreshAtom,
} from '@/atoms/preview-atoms'

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

  React.useEffect(() => {
    if (!filePath) return
    let unlistenFocus: (() => void) | undefined
    let unlistenWrite: (() => void) | undefined
    let cancelled = false

    void (async () => {
      const u1 = await listen('tauri://focus', () => {
        if (!cancelled) bump(filePath)
      })
      const u2 = await listen<FileWrittenPayload>('agent:file-written', (evt) => {
        if (cancelled) return
        if (evt.payload?.path === filePath) bump(filePath)
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

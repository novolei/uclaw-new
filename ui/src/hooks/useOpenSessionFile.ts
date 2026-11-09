/**
 * useOpenSessionFile — open a session-relative (or absolute) file path in the
 * preview panel. Resolves the path against the session's mounts via
 * `preview_resolve_chips`; paths outside every mount open in ad-hoc absolute
 * mode (matching ToolActivityItem's preview fallback). Best-effort: a path
 * that does not resolve is ignored.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'

interface ChipResolution {
  input: string
  exists: boolean
  mountId: string | null
  relPath: string | null
  absolutePath: string | null
}

export function pathBaseName(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? path
}

export function useOpenSessionFile(sessionId: string | null | undefined): (path: string) => void {
  const openPreviewTab = useSetAtom(openPreviewTabAction)
  return React.useCallback(
    (path: string) => {
      if (!sessionId) return
      void invoke<ChipResolution[]>('preview_resolve_chips', { paths: [path], sessionId })
        .then((results) => {
          const r = results?.[0]
          if (!r) return
          if (r.mountId && r.relPath && r.absolutePath) {
            openPreviewTab({
              target: {
                mountId: r.mountId,
                relPath: r.relPath,
                name: pathBaseName(r.relPath),
                sessionId,
                absolutePath: r.absolutePath,
              },
              source: 'manual',
            })
          } else if (r.absolutePath) {
            openPreviewTab({
              target: {
                mountId: 'workspace:default',
                relPath: r.absolutePath,
                name: pathBaseName(r.absolutePath),
                sessionId,
                absolutePath: r.absolutePath,
              },
              source: 'manual',
            })
          }
        })
        .catch(() => { /* best-effort */ })
    },
    [sessionId, openPreviewTab],
  )
}

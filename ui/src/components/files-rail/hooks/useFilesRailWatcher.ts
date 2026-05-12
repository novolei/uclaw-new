/**
 * useFilesRailWatcher — Subscribe to `files_rail:change` events for a mount
 * and apply them to the cached tree via tree-patch.
 */

import * as React from 'react'
import { listen } from '@tauri-apps/api/event'
import type { FileChange } from '@/components/files-rail/utils/tree-patch'

interface BackendFileChange {
  kind: 'created' | 'modified' | 'removed' | 'renamed'
  rel_path: string
  new_rel_path?: string | null
  is_dir: boolean
}

interface BackendFilesRailChange {
  mount_id: string
  changes: BackendFileChange[]
}

const toFileChange = (c: BackendFileChange): FileChange => ({
  kind: c.kind,
  relPath: c.rel_path,
  newRelPath: c.new_rel_path ?? undefined,
  isDir: c.is_dir,
})

export function useFilesRailWatcher(
  mountId: string,
  apply: (changes: FileChange[]) => void,
): void {
  React.useEffect(() => {
    let unlisten: (() => void) | undefined
    let cancelled = false

    void (async () => {
      const u = await listen<BackendFilesRailChange>('files_rail:change', (evt) => {
        if (cancelled) return
        if (evt.payload.mount_id !== mountId) return
        if (evt.payload.changes.length === 0) return
        apply(evt.payload.changes.map(toFileChange))
      })
      unlisten = u
    })()

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [mountId, apply])
}

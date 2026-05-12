/**
 * preview-panel-atoms — W4a preview panel state.
 *
 * selectedPreviewFileAtom — the file currently shown in the panel
 * previewPanelOpenAtom — whether the panel is visible
 * previewPanelWidthAtom — user-resizable, persisted
 * openPreviewAction — atomic set-file + open
 * closePreviewAction — convenience
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export interface PreviewFileTarget {
  /** Identifies the mount the file lives in (workspace:* / attached:*). */
  mountId: string
  /** Forward-slash path relative to the mount root. */
  relPath: string
  /** Display name (last segment of relPath). */
  name: string
  /** Optional session id — required for session-scoped mounts. */
  sessionId?: string | null
  /** Absolute on-disk path. Empty string if not yet resolved. */
  absolutePath?: string
}

export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(null)
export const previewPanelOpenAtom = atom<boolean>(false)

/** Persisted width in CSS pixels. Default 540; clamped to [380, 1100] by the UI. */
export const previewPanelWidthAtom = atomWithStorage<number>(
  'uclaw-preview-panel-width',
  540,
)

/** Write-only action: select a file AND open the panel in one update. */
export const openPreviewAction = atom(null, (_get, set, payload: PreviewFileTarget) => {
  set(selectedPreviewFileAtom, payload)
  set(previewPanelOpenAtom, true)
})

/** Write-only action: close the panel, keep the selection for re-open. */
export const closePreviewAction = atom(null, (_get, set) => {
  set(previewPanelOpenAtom, false)
})

/**
 * usePreviewState — Read-only convenience wrapper over the panel atoms.
 *
 * Components that only need to know the current target use this hook to
 * avoid each importing useAtomValue + the atom separately.
 */

import { useAtomValue } from 'jotai'
import {
  previewPanelOpenAtom,
  previewPanelWidthAtom,
  selectedPreviewFileAtom,
  type PreviewFileTarget,
} from '@/atoms/preview-panel-atoms'

export interface PreviewState {
  open: boolean
  width: number
  target: PreviewFileTarget | null
}

export function usePreviewState(): PreviewState {
  return {
    open: useAtomValue(previewPanelOpenAtom),
    width: useAtomValue(previewPanelWidthAtom),
    target: useAtomValue(selectedPreviewFileAtom),
  }
}

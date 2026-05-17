/**
 * Symphony Canvas UI state — viewport, selection, palette open state.
 * Stored separately from the data atoms so refetches don't disturb UX state.
 *
 * Scoping: viewport + selection are per-workflow, so each canvas tab keeps
 * its own pan/zoom/selection. We key into a Record<workflowId, …> and use
 * atomWithStorage so the user's zoom/pan survives reloads.
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type SymphonySubView = 'design' | 'run' | 'raw'

/** Top-tab strip: which sub-view is open. Default Design. */
export const symphonySubViewAtom = atomWithStorage<SymphonySubView>(
  'uclaw-symphony-subview',
  'design',
)

export interface CanvasViewport {
  x: number
  y: number
  zoom: number
}

/** Per-workflow viewport map. atomWithStorage preserves across reloads. */
export const symphonyViewportsAtom = atomWithStorage<
  Record<string, CanvasViewport>
>('uclaw-symphony-viewports', {})

/** Currently selected node id within the open workflow. */
export const symphonySelectedNodeIdAtom = atom<string | null>(null)

/** Whether the palette sidebar (Design view, left) is open. */
export const symphonyPaletteOpenAtom = atomWithStorage<boolean>(
  'uclaw-symphony-palette-open',
  true,
)

/** Whether the inspector sidebar (Design view, right) is open. */
export const symphonyInspectorOpenAtom = atomWithStorage<boolean>(
  'uclaw-symphony-inspector-open',
  true,
)

/**
 * preview-atoms — Wave 1 of the renderer quick-wins port.
 *
 * A per-file refresh counter. Anything that should trigger preview re-reads
 * (agent file-write events, window focus, manual refresh button, W3 files-rail
 * change events) bumps the relevant file's counter. The code-highlight cache
 * keys include the version, so a bump naturally invalidates cache entries.
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'

export const previewRefreshVersionAtomFamily = atomFamily((_filePath: string) => atom(0))

/** Bump the refresh counter for one file. Pass the file path as the action payload. */
export const bumpPreviewRefreshAtom = atom(null, (get, set, filePath: string) => {
  const a = previewRefreshVersionAtomFamily(filePath)
  set(a, get(a) + 1)
})

/** Reset all known files' versions to 0. Used by tests; also safe at logout/workspace switch. */
export const resetAllPreviewRefreshAtom = atom(null, (_get, _set) => {
  // atomFamily.getParams() lists previously-accessed params
  for (const p of previewRefreshVersionAtomFamily.getParams()) {
    previewRefreshVersionAtomFamily.remove(p)
  }
})

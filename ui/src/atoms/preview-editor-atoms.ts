/**
 * preview-editor-atoms — Shared state for preview editing surfaces.
 *
 * Three atoms power the editor stack:
 *
 *   - dirtyBuffersAtom: Map<filePath, DirtyBuffer>
 *       Both code-mode (explicit save) AND markdown-mode (auto-save)
 *       editors register here. Acts as the SINGLE SOURCE OF TRUTH for
 *       "this file has unsaved local edits" — read by:
 *         - usePreviewRefresh (skips refetch bumps for dirty files)
 *         - openPreviewAction / closePreviewAction (confirm-on-close)
 *         - beforeunload guard inside useDirtyBuffer
 *
 *   - isDirtyAtomFamily(filePath) → boolean
 *       Selector form of `dirtyBuffersAtom.has(filePath)`. Use this in
 *       hot paths so consumers don't have to re-render whenever any other
 *       file's dirty state flips.
 *
 *   - markdownEditorModeAtom: 'rich' | 'raw'  (persisted via atomWithStorage)
 *   - tipTapFidelityToastShownAtom: boolean (persisted)
 *
 * Mtime-based optimistic concurrency control was REMOVED (2026-05-13).
 * Reason: it kept producing false-positive "file changed on disk" warnings
 * against macOS's coarse mtime resolution and React's commit timing window.
 * The simpler dirty-guard pattern (cribbed from if2Ai's preview panel)
 * eliminates the entire class of race by design — see preview/commands.rs
 * and usePreviewRefresh.ts.
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import { atomWithStorage } from 'jotai/utils'

export interface DirtyBuffer {
  filePath: string
  content: string
  baselineMtimeMs: number
}

/** Map of currently-dirty buffers (any save mode). */
export const dirtyBuffersAtom = atom<Map<string, DirtyBuffer>>(new Map())

/**
 * Per-path selector for "is this file currently dirty". Cheaper than
 * subscribing to the whole Map — only re-fires when this path's
 * membership changes, not on every other file's edits.
 */
export const isDirtyAtomFamily = atomFamily((filePath: string) =>
  atom((get) => get(dirtyBuffersAtom).has(filePath)),
)

/**
 * Markdown editor mode toggle — persisted across sessions.
 * 'rich' = TipTap WYSIWYG; 'raw' = CodeMirror source.
 */
export const markdownEditorModeAtom = atomWithStorage<'rich' | 'raw'>(
  'uclaw-md-editor-mode',
  'rich',
)

/** One-time toast shown when the user first edits a markdown file in
 *  rich mode this session. Suppressible. */
export const tipTapFidelityToastShownAtom = atomWithStorage<boolean>(
  'uclaw-tiptap-fidelity-warning-shown',
  false,
)

// ─── Write atoms (action helpers) ─────────────────────────────────────

/** Register or update a dirty buffer. */
export const setDirtyBufferAction = atom(
  null,
  (get, set, buf: DirtyBuffer) => {
    const next = new Map(get(dirtyBuffersAtom))
    next.set(buf.filePath, buf)
    set(dirtyBuffersAtom, next)
  },
)

/** Clear a dirty buffer (called on successful save or content-returns-to-baseline). */
export const clearDirtyBufferAction = atom(
  null,
  (get, set, filePath: string) => {
    const cur = get(dirtyBuffersAtom)
    if (!cur.has(filePath)) return
    const next = new Map(cur)
    next.delete(filePath)
    set(dirtyBuffersAtom, next)
  },
)

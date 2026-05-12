/**
 * preview-editor-atoms — Shared state for W4d preview editing surfaces.
 *
 * Five atoms power the editor stack:
 *
 *   - dirtyBuffersAtom: Map<filePath, DirtyBuffer>
 *       Only used by code-mode editors (explicit save). Markdown editors
 *       use auto-save and never register here.
 *   - markdownEditorModeAtom: 'rich' | 'raw'  (persisted via atomWithStorage)
 *   - conflictsAtom: Map<filePath, ExternalConflict>
 *       Populated when preview_write_text returns Conflict; auto-save
 *       pauses per-filePath while a conflict exists.
 *   - lastSelfWriteMtimeAtom: Map<filePath, number>
 *       Self-write echo guard. Editor adds the mtime returned by Saved;
 *       file-watcher subscriptions filter Modified events whose mtime
 *       matches exactly (those are our own writes).
 *   - tipTapFidelityToastShownAtom: boolean (persisted)
 *       True after the user has seen the one-time fidelity warning when
 *       first editing in TipTap rich mode this session.
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export interface DirtyBuffer {
  filePath: string
  content: string
  baselineMtimeMs: number
}

export interface ExternalConflict {
  externalContent: string
  externalMtimeMs: number
}

/** Map of currently-dirty buffers (explicit-save / code mode only). */
export const dirtyBuffersAtom = atom<Map<string, DirtyBuffer>>(new Map())

/**
 * Markdown editor mode toggle — persisted across sessions.
 * 'rich' = TipTap WYSIWYG; 'raw' = CodeMirror source.
 */
export const markdownEditorModeAtom = atomWithStorage<'rich' | 'raw'>(
  'uclaw-md-editor-mode',
  'rich',
)

/** Map of currently-pending external conflicts (one per filePath). */
export const conflictsAtom = atom<Map<string, ExternalConflict>>(new Map())

/**
 * Per-filePath last-self-write mtime — used to filter the editor's OWN
 * writes out of the file-watcher's Modified events stream. When the
 * watcher fires with mtime === lastSelfWriteMtime, ignore it; otherwise
 * treat as external change.
 */
export const lastSelfWriteMtimeAtom = atom<Map<string, number>>(new Map())

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

/** Clear a dirty buffer (called on successful save). */
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

/** Set an external conflict (called after a Conflict response). */
export const setConflictAction = atom(
  null,
  (get, set, payload: { filePath: string; conflict: ExternalConflict }) => {
    const next = new Map(get(conflictsAtom))
    next.set(payload.filePath, payload.conflict)
    set(conflictsAtom, next)
  },
)

/** Clear a conflict (called when user resolves via Overwrite/Discard/✕). */
export const clearConflictAction = atom(
  null,
  (get, set, filePath: string) => {
    const cur = get(conflictsAtom)
    if (!cur.has(filePath)) return
    const next = new Map(cur)
    next.delete(filePath)
    set(conflictsAtom, next)
  },
)

/** Record a self-write mtime (called after Saved). */
export const recordSelfWriteAction = atom(
  null,
  (get, set, payload: { filePath: string; mtimeMs: number }) => {
    const next = new Map(get(lastSelfWriteMtimeAtom))
    next.set(payload.filePath, payload.mtimeMs)
    set(lastSelfWriteMtimeAtom, next)
  },
)

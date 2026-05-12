/**
 * files-rail-row-atoms — single-target atoms for row-level UI state.
 *
 * Only one rename / move / delete can be in flight at a time. The atoms
 * are nullable; setting to non-null opens the corresponding UI (inline
 * rename input, MoveToDialog, DeleteConfirmDialog). Setting back to
 * null closes / commits / cancels.
 *
 * Targets carry the absolute path (canonical identity for files-rail
 * rows) plus enough metadata for the dialogs to render their copy
 * without re-walking the tree.
 */

import { atom } from 'jotai'

export interface FileRowTarget {
  mountId: string
  absolutePath: string
  /** Workspace-relative path (sans leading slash) for IPC calls. */
  workspaceRelPath: string
  name: string
  isDirectory: boolean
}

/**
 * When non-null, the FileTreeNode at this absolutePath renders RenameInput.
 * Stored as a bare path (not a FileRowTarget) because the rename input is
 * inline inside the row that already has the node's metadata in scope —
 * no dialog needs to be populated from this atom.
 */
export const renamingFilePathAtom = atom<string | null>(null)

/** When non-null, MoveToDialog is open for this target. */
export const moveTargetAtom = atom<FileRowTarget | null>(null)

/** When non-null, DeleteConfirmDialog is open for this target. */
export const deleteTargetAtom = atom<FileRowTarget | null>(null)

/**
 * files-rail-atoms — State for the W3 files rail.
 *
 * mountRootsAtomFamily — list of mounts for a given sessionId
 * expandedPathsAtomFamily — per-mount Set<relPath> of expanded directories
 * fileTreeAtomFamily — per-mount root tree (TreeState)
 * filesRailTabAtom — workspace | changes
 * filesRailRefreshTickAtom — bump to force a full reload of mounts
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

export type FilesRailTab = 'workspace' | 'changes'
export type MountKind = 'workspace' | 'session' | 'attached_dir'

export interface MountRoot {
  id: string
  label: string
  path: string
  kind: MountKind
  editable: boolean
}

/** Loadable wrapper for a per-mount tree. */
export type TreeState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; nodes: TreeNode[] }
  | { status: 'error'; message: string }

export const filesRailTabAtom = atom<FilesRailTab>('workspace')

export const mountRootsAtomFamily = atomFamily(
  (_sessionId: string | null) => atom<MountRoot[]>([]),
)

export const expandedPathsAtomFamily = atomFamily((_mountId: string) =>
  atom<Set<string>>(new Set<string>()),
)

export const fileTreeAtomFamily = atomFamily((_mountId: string) =>
  atom<TreeState>({ status: 'idle' }),
)

export const filesRailRefreshTickAtom = atom(0)
export const bumpFilesRailRefreshAtom = atom(null, (get, set) => {
  set(filesRailRefreshTickAtom, get(filesRailRefreshTickAtom) + 1)
})

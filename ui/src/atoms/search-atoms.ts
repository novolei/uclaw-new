/**
 * 搜索 Dialog 状态 Atoms
 *
 * 管理全局搜索 Dialog 的开关、查询词和搜索结果。
 */

import { atom } from 'jotai'

/** 搜索 Dialog 是否打开 */
export const searchDialogOpenAtom = atom(false)

/** Whether the global search palette is currently open. */
export const searchPaletteOpenAtom = atom<boolean>(false)

/**
 * Search-palette scope. `'all'` (default) does a global FTS; an object
 * limits the search to one conversation / agent session.
 *
 * The palette renders a chip when scope is non-default and supports
 * `Tab` (set to current session) / `Esc` (clear scope, then close).
 */
export type SearchScope =
  | 'all'
  | { kind: 'session'; id: string; label: string }

export const searchPaletteScopeAtom = atom<SearchScope>('all')

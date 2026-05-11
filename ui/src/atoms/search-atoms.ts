/**
 * 搜索 Palette 状态 Atoms
 *
 * 管理全局搜索 Palette 的开关、范围和查询词。
 */

import { atom } from 'jotai'

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

import { atomWithStorage } from 'jotai/utils'
import { atom } from 'jotai'
import { arrayMove } from '@dnd-kit/sortable'

/** Persisted to localStorage; default off so Dock only shows when user opts in. */
export const bottomDockEnabledAtom = atomWithStorage('dock:enabled', false)

/** Mirrors navigator.onLine + online/offline events. */
export const internetOnlineAtom = atom(true)

/** True when get_app_health Tauri invoke succeeds. */
export const backendOnlineAtom = atom(true)

/**
 * null = not yet polled (initializing)
 * true = memU bridge alive
 * false = bridge offline or not initialized
 */
export const memuOnlineAtom = atom<boolean | null>(null)

/**
 * Phase 2 data model — drives BottomDock's render and reorder.
 *
 * Each entry is a discriminated union. Phase 2A renders only `kind: 'mode'`
 * entries; Phase 2B adds rendering + source-side context menus for the
 * three `pinned-*` variants.
 *
 * Seeded with the 4 modes in the original Phase 1 order; reorder + pin
 * mutations write back through `atomWithStorage`, surviving app restarts.
 */
export type DockItemSpec =
  | { kind: 'mode'; mode: 'chat' | 'agent' | 'memory' | 'kaleidoscope' }
  | { kind: 'pinned-conversation'; sessionId: string; type: 'chat' | 'agent' }
  | { kind: 'pinned-workspace'; spaceId: string }
  | { kind: 'pinned-automation'; specId: string }

export const dockOrderAtom = atomWithStorage<DockItemSpec[]>('dock:order', [
  { kind: 'mode', mode: 'chat' },
  { kind: 'mode', mode: 'agent' },
  { kind: 'mode', mode: 'memory' },
  { kind: 'mode', mode: 'kaleidoscope' },
])

/**
 * Pure reorder helper. Given the current dock order, the stable id list
 * fed to SortableContext, and the dnd-kit active/over ids from a DragEnd
 * event, return the new dock order (or the same array if no movement).
 *
 * Extracted so the BottomDock controller stays thin and the logic is
 * unit-testable without a full DOM dance.
 */
export function applyDockReorder(
  current: DockItemSpec[],
  sortableIds: string[],
  activeId: string,
  overId: string,
): DockItemSpec[] {
  if (activeId === overId) return current
  const oldIndex = sortableIds.indexOf(activeId)
  const newIndex = sortableIds.indexOf(overId)
  if (oldIndex < 0 || newIndex < 0) return current
  return arrayMove(current, oldIndex, newIndex)
}

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

/**
 * Returns the sortable id for a pinned-* spec. Mirrors specToSortableId
 * in BottomDock.tsx but lives here so add/remove helpers can match by id.
 * Mode-* ids are not produced here (modes are not pinnable).
 */
function pinnedSpecSortableId(spec: DockItemSpec): string | null {
  switch (spec.kind) {
    case 'pinned-conversation':
      return `conv-${spec.sessionId}`
    case 'pinned-workspace':
      return `space-${spec.spaceId}`
    case 'pinned-automation':
      return `auto-${spec.specId}`
    default:
      return null
  }
}

/**
 * Append a pin to the dock order. Idempotent — if a spec with the same
 * sortable id is already present, returns the input array unchanged
 * (referential equality). Modes are not pinnable; the type system
 * already enforces that (caller passes a pinned-* spec).
 */
export function addDockPin(
  current: DockItemSpec[],
  spec: Exclude<DockItemSpec, { kind: 'mode' }>,
): DockItemSpec[] {
  const newId = pinnedSpecSortableId(spec)
  if (newId === null) return current
  const exists = current.some((s) => pinnedSpecSortableId(s) === newId)
  if (exists) return current
  return [...current, spec]
}

/**
 * Remove a pin by its sortable id (e.g. 'space-workspace-1', 'conv-sess-1',
 * 'auto-spec-1'). Returns the input array unchanged (referential equality)
 * when no entry matches — including any attempt to remove a mode-* id.
 */
export function removeDockPin(
  current: DockItemSpec[],
  sortableId: string,
): DockItemSpec[] {
  const idx = current.findIndex((s) => pinnedSpecSortableId(s) === sortableId)
  if (idx < 0) return current
  return [...current.slice(0, idx), ...current.slice(idx + 1)]
}

/**
 * Deterministic 2-color HSL gradient seeded from a string id. Used by
 * DockPinnedItem when the entity has no explicit color (most pinned
 * conversations / automation specs). FNV-1a hash → hue 20..339 (skip
 * yellow-grey band), 70% saturation, lightness 55→45 with +20° hue walk.
 */
export function pinIdColorSeed(id: string): { from: string; to: string } {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  const hue = ((h >>> 0) % 320) + 20
  const sat = 70
  const lightFrom = 55
  const lightTo = 45
  return {
    from: `hsl(${hue}, ${sat}%, ${lightFrom}%)`,
    to: `hsl(${(hue + 20) % 360}, ${sat}%, ${lightTo}%)`,
  }
}

/**
 * Phase 2C bounce signal — per-target monotonic counter keyed by sortable id
 * (e.g. 'mode-agent', 'mode-chat', or future pinned-* ids). Consumers
 * (DockItem, DockPinnedItem) compare against their previous value to detect
 * "should run a one-shot bounce animation now". Resetting to 0 is unnecessary —
 * the counter is read incrementally, not absolutely.
 *
 * Intentionally NOT persisted: bounces are transient attention signals, not
 * state to remember across reloads.
 */
export const dockBounceKeysAtom = atom<Record<string, number>>({})

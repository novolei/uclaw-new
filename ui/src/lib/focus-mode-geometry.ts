/**
 * focus-mode-geometry — pure helpers for the Focus Mode overlay
 * geometry. Kept here (not inside the hotzone hook) so they can be
 * unit-tested in isolation and reused by FloatingIsland for layout.
 */

/** Width of the LEFT floating island in CSS px (mirrors LeftSidebar default). */
export const ISLAND_LEFT_WIDTH = 280
/** Width of the RIGHT floating island in CSS px (mirrors RightSidePanel fixed width). */
export const ISLAND_RIGHT_WIDTH = 400
/** Gap between island edge and screen edge. */
export const ISLAND_EDGE_GAP = 12
/** Top band reserved for the titlebar drag region + TabBar — hot zone is excluded here. */
export const TOP_EXCLUDE = 84
/** Hot zone width near the screen edge (in CSS px). */
export const HOT_ZONE_WIDTH = 32

/**
 * Returns true if `(x, y)` falls inside the bounding box of the floating
 * island on `side`. Used by the hotzone hook to decide whether the mouse
 * is still "in the island region" (which suppresses the leave timer).
 *
 * Top band (y < TOP_EXCLUDE) is always rejected so the hot zone never
 * fights with the macOS traffic-light buttons / window drag.
 */
export function isInsideIslandRect(
  side: 'left' | 'right',
  x: number,
  y: number,
  windowWidth: number,
  windowHeight: number,
): boolean {
  if (y < TOP_EXCLUDE) return false
  if (y > windowHeight - ISLAND_EDGE_GAP) return false
  if (side === 'left') {
    return x >= ISLAND_EDGE_GAP && x <= ISLAND_EDGE_GAP + ISLAND_LEFT_WIDTH
  }
  const rightStart = windowWidth - ISLAND_EDGE_GAP - ISLAND_RIGHT_WIDTH
  const rightEnd = windowWidth - ISLAND_EDGE_GAP
  return x >= rightStart && x <= rightEnd
}

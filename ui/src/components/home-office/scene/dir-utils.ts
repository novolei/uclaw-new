import type { Direction, Vec2 } from '@/atoms/home-office-atoms'

/**
 * Convert a 2D vector (e.g. target - current position) into the closest of 8
 * compass directions. Zero vector falls back to 'S' (default facing).
 */
export function vectorToDirection(v: Vec2): Direction {
  const { x, y } = v
  if (x === 0 && y === 0) return 'S'
  // atan2 with screen-coords: y grows downward, so 'S' is angle +90°.
  const angle = Math.atan2(y, x) // radians, -π..π
  const deg = (angle * 180) / Math.PI // -180..180
  // Snap to nearest 45° bucket
  const bucket = Math.round(deg / 45)
  switch (((bucket % 8) + 8) % 8) {
    case 0: return 'E'
    case 1: return 'SE'
    case 2: return 'S'
    case 3: return 'SW'
    case 4: return 'W'
    case 5: return 'NW'
    case 6: return 'N'
    case 7: return 'NE'
  }
  return 'S'
}

/**
 * Resolve a Direction to an asset key + horizontal flip flag.
 * Mirrors E/NE/SE off W/NW/SW so we only ship 5 walk WebPs per character.
 */
export function resolveSpriteKey(d: Direction): { key: string; flipX: boolean } {
  switch (d) {
    case 'E':  return { key: 'walk-W',  flipX: true }
    case 'NE': return { key: 'walk-NW', flipX: true }
    case 'SE': return { key: 'walk-SW', flipX: true }
    case 'W':  return { key: 'walk-W',  flipX: false }
    case 'NW': return { key: 'walk-NW', flipX: false }
    case 'SW': return { key: 'walk-SW', flipX: false }
    case 'N':  return { key: 'walk-N',  flipX: false }
    case 'S':  return { key: 'walk-S',  flipX: false }
  }
}

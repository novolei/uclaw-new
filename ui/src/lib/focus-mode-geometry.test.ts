import { describe, it, expect } from 'vitest'
import {
  isInsideIslandRect,
  ISLAND_LEFT_WIDTH,
  ISLAND_RIGHT_WIDTH,
  ISLAND_EDGE_GAP,
  TOP_EXCLUDE,
} from './focus-mode-geometry'

const W = 1440
const H = 900

describe('isInsideIslandRect', () => {
  it('returns true for a point inside the LEFT island box', () => {
    // Left island: x in [12, 12+280=292], y in [12, 900-12=888]
    expect(isInsideIslandRect('left', 150, 400, W, H)).toBe(true)
  })

  it('returns false for a point outside the LEFT island (to the right of it)', () => {
    expect(isInsideIslandRect('left', 400, 400, W, H)).toBe(false)
  })

  it('returns true for a point inside the RIGHT island box', () => {
    // Right island: x in [W-12-400=1028, W-12=1428]
    expect(isInsideIslandRect('right', 1200, 400, W, H)).toBe(true)
  })

  it('returns false for the WRONG side (mouse on right but checking left)', () => {
    expect(isInsideIslandRect('left', 1200, 400, W, H)).toBe(false)
  })

  it('excludes the top TOP_EXCLUDE band', () => {
    // y < 84 should never count
    expect(isInsideIslandRect('left', 150, 50, W, H)).toBe(false)
    expect(isInsideIslandRect('right', 1200, 50, W, H)).toBe(false)
  })

  it('exposes geometry constants used by the overlay layout', () => {
    expect(ISLAND_LEFT_WIDTH).toBe(280)
    expect(ISLAND_RIGHT_WIDTH).toBe(400)
    expect(ISLAND_EDGE_GAP).toBe(12)
    expect(TOP_EXCLUDE).toBe(84)
  })
})

import { describe, it, expect } from 'vitest'
import { vectorToDirection, resolveSpriteKey } from './dir-utils'

describe('vectorToDirection', () => {
  it('returns S for downward vector', () => {
    expect(vectorToDirection({ x: 0, y: 1 })).toBe('S')
  })

  it('returns N for upward vector', () => {
    expect(vectorToDirection({ x: 0, y: -1 })).toBe('N')
  })

  it('returns E for rightward vector', () => {
    expect(vectorToDirection({ x: 1, y: 0 })).toBe('E')
  })

  it('returns W for leftward vector', () => {
    expect(vectorToDirection({ x: -1, y: 0 })).toBe('W')
  })

  it('returns NE for up-right diagonal', () => {
    expect(vectorToDirection({ x: 0.7, y: -0.7 })).toBe('NE')
  })

  it('returns SW for down-left diagonal', () => {
    expect(vectorToDirection({ x: -0.7, y: 0.7 })).toBe('SW')
  })

  it('returns S for zero vector (default)', () => {
    expect(vectorToDirection({ x: 0, y: 0 })).toBe('S')
  })
})

describe('resolveSpriteKey (mirror optimization)', () => {
  it('uses walk-W for W', () => {
    expect(resolveSpriteKey('W')).toEqual({ key: 'walk-W', flipX: false })
  })

  it('mirrors W asset for E', () => {
    expect(resolveSpriteKey('E')).toEqual({ key: 'walk-W', flipX: true })
  })

  it('uses walk-NW for NW', () => {
    expect(resolveSpriteKey('NW')).toEqual({ key: 'walk-NW', flipX: false })
  })

  it('mirrors NW asset for NE', () => {
    expect(resolveSpriteKey('NE')).toEqual({ key: 'walk-NW', flipX: true })
  })

  it('mirrors SW asset for SE', () => {
    expect(resolveSpriteKey('SE')).toEqual({ key: 'walk-SW', flipX: true })
  })

  it('uses walk-N for N (no mirror)', () => {
    expect(resolveSpriteKey('N')).toEqual({ key: 'walk-N', flipX: false })
  })

  it('uses walk-S for S (no mirror)', () => {
    expect(resolveSpriteKey('S')).toEqual({ key: 'walk-S', flipX: false })
  })
})

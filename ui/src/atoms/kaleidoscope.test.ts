import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { topLevelViewAtom } from './top-level-view'
import {
  kaleidoscopeModuleAtom,
  KALEIDOSCOPE_MODULES,
} from './kaleidoscope'

describe('top-level-view / kaleidoscope atoms', () => {
  it('topLevelViewAtom defaults to "workspace"', () => {
    const store = createStore()
    expect(store.get(topLevelViewAtom)).toBe('workspace')
  })

  it('kaleidoscopeModuleAtom defaults to "humans"', () => {
    const store = createStore()
    expect(store.get(kaleidoscopeModuleAtom)).toBe('humans')
  })

  it('KALEIDOSCOPE_MODULES lists 7 modules: 4 asset + 3 capability', () => {
    expect(KALEIDOSCOPE_MODULES).toHaveLength(7)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'asset')).toHaveLength(4)
    expect(KALEIDOSCOPE_MODULES.filter((m) => m.group === 'capability')).toHaveLength(3)
  })

  it('every module id is unique and humans is first', () => {
    const ids = KALEIDOSCOPE_MODULES.map((m) => m.id)
    expect(new Set(ids).size).toBe(7)
    expect(ids[0]).toBe('humans')
  })
})

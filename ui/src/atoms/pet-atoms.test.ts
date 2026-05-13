import { createStore } from 'jotai'
import { afterEach, describe, expect, it } from 'vitest'
import {
  petCharacterAtom,
  petDisplayStateAtom,
  petEnabledAtom,
  petHoverActiveAtom,
  petPrimaryStateAtom,
} from './pet-atoms'

describe('pet-atoms', () => {
  afterEach(() => {
    localStorage.clear()
  })

  it('defaults to disabled and Astro', () => {
    const store = createStore()
    expect(store.get(petEnabledAtom)).toBe(false)
    expect(store.get(petCharacterAtom)).toBe('astro')
  })

  it('primary state defaults to idle, hover defaults off', () => {
    const store = createStore()
    expect(store.get(petPrimaryStateAtom)).toBe('idle')
    expect(store.get(petHoverActiveAtom)).toBe(false)
    expect(store.get(petDisplayStateAtom)).toBe('idle')
  })

  it('display state returns hover when primary is idle and hover active', () => {
    const store = createStore()
    store.set(petHoverActiveAtom, true)
    expect(store.get(petDisplayStateAtom)).toBe('hover')
  })

  it.each(['thinking', 'typing', 'success', 'error'] as const)(
    'hover does NOT override primary state %s',
    (primary) => {
      const store = createStore()
      store.set(petPrimaryStateAtom, primary)
      store.set(petHoverActiveAtom, true)
      expect(store.get(petDisplayStateAtom)).toBe(primary)
    },
  )

  it('hover false returns primary unchanged', () => {
    const store = createStore()
    store.set(petPrimaryStateAtom, 'thinking')
    store.set(petHoverActiveAtom, false)
    expect(store.get(petDisplayStateAtom)).toBe('thinking')
  })
})

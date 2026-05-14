import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import {
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
} from '@/atoms/home-office-atoms'
import { useCharacterPath } from './useCharacterPath'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useCharacterPath', () => {
  beforeEach(() => { vi.useFakeTimers() })
  afterEach(() => { vi.useRealTimers() })

  it('starts walking when state changes to thinking', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.5, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    expect(store.get(characterMotionAtom)).toBe('walk')
  })

  it('sets direction toward library tower for thinking', () => {
    const store = createStore()
    // start near oak desk
    store.set(characterPositionAtom, { x: 0.50, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    // library is at (0.68, 0.22) — up and to the right → NE
    expect(store.get(characterDirectionAtom)).toBe('NE')
  })

  it('reaches target and switches to pose', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.50, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    // Run ticker for plenty of time
    act(() => {
      vi.advanceTimersByTime(10_000)
    })
    expect(store.get(characterMotionAtom)).toBe('pose')
    const pos = store.get(characterPositionAtom)
    // close to library zone (0.68, 0.22)
    expect(Math.abs(pos.x - 0.68)).toBeLessThan(0.01)
    expect(Math.abs(pos.y - 0.22)).toBeLessThan(0.01)
  })

  it('success state stays in place (no walk)', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.40, y: 0.60 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'success')
    })
    expect(store.get(characterMotionAtom)).toBe('pose')
    const pos = store.get(characterPositionAtom)
    expect(pos).toEqual({ x: 0.40, y: 0.60 })
  })
})

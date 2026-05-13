import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
  toggleFocusModeAction,
  exitFocusModeAction,
} from './focus-mode-atoms'

describe('focus-mode-atoms', () => {
  it('defaults to off / null / unpinned / origin', () => {
    const store = createStore()
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
    expect(store.get(focusMousePosAtom)).toEqual({ x: 0, y: 0 })
  })

  it('toggleFocusModeAction flips focusModeAtom', () => {
    const store = createStore()
    store.set(toggleFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(true)
    store.set(toggleFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(false)
  })

  it('toggling OFF clears reveal + pin state', () => {
    const store = createStore()
    store.set(toggleFocusModeAction)             // → on
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    store.set(toggleFocusModeAction)             // → off, must clean up
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })

  it('exitFocusModeAction forces every flag back to defaults', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'right')
    store.set(focusRevealPinnedAtom, true)
    store.set(exitFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })
})

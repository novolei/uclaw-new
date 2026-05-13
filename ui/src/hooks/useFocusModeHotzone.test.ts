import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useFocusModeHotzone } from './useFocusModeHotzone'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

function fireMouseMove(clientX: number, clientY: number): void {
  window.dispatchEvent(new MouseEvent('mousemove', { clientX, clientY }))
}

const ORIG_W = window.innerWidth
const ORIG_H = window.innerHeight

function setViewport(w: number, h: number): void {
  Object.defineProperty(window, 'innerWidth', { value: w, writable: true, configurable: true })
  Object.defineProperty(window, 'innerHeight', { value: h, writable: true, configurable: true })
}

describe('useFocusModeHotzone', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setViewport(1440, 900)
  })
  afterEach(() => {
    vi.useRealTimers()
    setViewport(ORIG_W, ORIG_H)
  })

  it('reveals left when mouse enters left hot zone (x <= 8, y > 84)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(5, 200))
    expect(store.get(focusRevealSideAtom)).toBe('left')
  })

  it('reveals right when mouse enters right hot zone', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(1435, 200))   // 1440 - 5 = right hot zone
    expect(store.get(focusRevealSideAtom)).toBe('right')
  })

  it('does NOT reveal when mouse is in the glow band but outside the hot zone', () => {
    // The new HOT_ZONE_WIDTH = 8; the glow band visually starts at 160px
    // but should not yet trigger the island. Hovering at x=20 (glow-visible
    // but past the 8px hot zone) must NOT slide the island in.
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 200))
    expect(store.get(focusRevealSideAtom)).toBeNull()
  })

  it('does NOT reveal when y < TOP_EXCLUDE (84)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(5, 50))       // in left hot zone X-wise, but y < 84
    expect(store.get(focusRevealSideAtom)).toBeNull()
  })

  it('starts 200ms leave timer when mouse leaves the island/hot-zone region', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(5, 200))          // reveal left
    expect(store.get(focusRevealSideAtom)).toBe('left')
    act(() => fireMouseMove(600, 200))        // mouse leaves region
    expect(store.get(focusRevealSideAtom)).toBe('left')  // not yet
    act(() => vi.advanceTimersByTime(200))
    expect(store.get(focusRevealSideAtom)).toBeNull()
  })

  it('cancels the leave timer if mouse returns to the region in time', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(5, 200))
    act(() => fireMouseMove(600, 200))        // leave region; 200ms timer starts
    act(() => vi.advanceTimersByTime(100))
    act(() => fireMouseMove(150, 200))        // back inside island bounding box
    act(() => vi.advanceTimersByTime(200))    // full window passes
    expect(store.get(focusRevealSideAtom)).toBe('left')  // still revealed
  })

  it('pinned state prevents the leave timer from hiding the island', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(600, 200))        // far from any reveal region
    act(() => vi.advanceTimersByTime(500))
    expect(store.get(focusRevealSideAtom)).toBe('left')  // pinned holds
  })

  it('updates focusMousePosAtom on every mousemove (drives glow)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(500, 300))
    expect(store.get(focusMousePosAtom)).toEqual({ x: 500, y: 300 })
  })

  it('does nothing when Focus Mode is OFF', () => {
    const store = createStore()
    store.set(focusModeAtom, false)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(5, 200))
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusMousePosAtom)).toEqual({ x: 0, y: 0 })
  })
})

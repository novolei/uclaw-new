import { describe, it, expect } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useFocusModeAutoExit } from './useFocusModeAutoExit'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useFocusModeAutoExit', () => {
  it('exits Focus Mode when preview closes', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(previewPanelOpenAtom, true)
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(true)
    act(() => store.set(previewPanelOpenAtom, false))
    expect(store.get(focusModeAtom)).toBe(false)
  })

  it('does not exit while preview stays open', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(previewPanelOpenAtom, true)
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(true)
  })

  it('corrects orphan focus state on mount (focus=true but no preview)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    store.set(previewPanelOpenAtom, false)   // orphan: focus on but no preview
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })
})

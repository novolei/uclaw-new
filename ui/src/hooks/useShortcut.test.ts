/**
 * useShortcut — focused tests for the modifier-key matching, especially
 * the Mac Option+letter quirk that historically caused Alt+F to never
 * fire on macOS (Option+F produces e.key='ƒ', not 'f').
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { renderHook } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useShortcut } from './useShortcut'
import { SHORTCUT_DEFINITIONS } from '@/lib/shortcut-defaults'

// We need to be sure the registry has 'toggle-focus-mode' wired to Alt+F
// before exercising matchesShortcut through useShortcut's path.
const ENTRY = SHORTCUT_DEFINITIONS.find((d) => d.id === 'toggle-focus-mode')

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

function fireKeyDown(opts: {
  key: string
  code: string
  altKey?: boolean
  metaKey?: boolean
  ctrlKey?: boolean
  shiftKey?: boolean
}): void {
  window.dispatchEvent(new KeyboardEvent('keydown', { ...opts }))
}

describe('useShortcut — Alt+F (Mac Option+F)', () => {
  let handler: ReturnType<typeof vi.fn>
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    handler = vi.fn()
    store = createStore()
  })

  it('fires when Mac dispatches Option+F (e.key="ƒ" + e.code="KeyF")', () => {
    expect(ENTRY).toBeDefined() // sanity — the binding must exist in registry
    renderHook(() => useShortcut({ id: 'toggle-focus-mode', handler }), {
      wrapper: wrapper(store),
    })
    // The real-world Mac event: altKey true, key is the special char ƒ, code is the physical key.
    fireKeyDown({ key: 'ƒ', code: 'KeyF', altKey: true })
    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('also fires when Windows / external keyboard dispatches plain Alt+F (e.key="f" + e.code="KeyF")', () => {
    renderHook(() => useShortcut({ id: 'toggle-focus-mode', handler }), {
      wrapper: wrapper(store),
    })
    fireKeyDown({ key: 'f', code: 'KeyF', altKey: true })
    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('does not fire when Alt is NOT pressed (avoids accidental "f" press triggering)', () => {
    renderHook(() => useShortcut({ id: 'toggle-focus-mode', handler }), {
      wrapper: wrapper(store),
    })
    fireKeyDown({ key: 'f', code: 'KeyF', altKey: false })
    expect(handler).not.toHaveBeenCalled()
  })

  it('does not fire when a different physical key is pressed with Alt (e.g. Alt+G)', () => {
    renderHook(() => useShortcut({ id: 'toggle-focus-mode', handler }), {
      wrapper: wrapper(store),
    })
    // Mac Option+G produces 'ˆ' on US layout. Code is KeyG. Must NOT match Alt+F.
    fireKeyDown({ key: 'ˆ', code: 'KeyG', altKey: true })
    expect(handler).not.toHaveBeenCalled()
  })
})

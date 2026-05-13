import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'
import { useShortcutCapture, eventToShortcut } from './useShortcutCapture'

function fireKey(opts: {
  code: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  shiftKey?: boolean
}): void {
  window.dispatchEvent(new KeyboardEvent('keydown', { ...opts, bubbles: true }))
}

describe('eventToShortcut — pure translator', () => {
  it('returns null for modifier-only presses', () => {
    expect(
      eventToShortcut(new KeyboardEvent('keydown', { code: 'MetaLeft', metaKey: true })),
    ).toBeNull()
    expect(
      eventToShortcut(new KeyboardEvent('keydown', { code: 'ShiftLeft', shiftKey: true })),
    ).toBeNull()
    expect(
      eventToShortcut(new KeyboardEvent('keydown', { code: 'AltLeft', altKey: true })),
    ).toBeNull()
  })

  it('builds Cmd+Shift+P from a real event', () => {
    expect(
      eventToShortcut(
        new KeyboardEvent('keydown', { code: 'KeyP', metaKey: true, shiftKey: true }),
      ),
    ).toBe('Cmd+Shift+P')
  })

  it('builds Alt+F from a Mac Option+F event (e.key would be ƒ, irrelevant)', () => {
    expect(
      eventToShortcut(
        new KeyboardEvent('keydown', { code: 'KeyF', altKey: true, key: 'ƒ' }),
      ),
    ).toBe('Alt+F')
  })

  it('builds Ctrl+/ from a punctuation key', () => {
    expect(
      eventToShortcut(new KeyboardEvent('keydown', { code: 'Slash', ctrlKey: true })),
    ).toBe('Ctrl+/')
  })

  it('returns null for unrecognized codes (e.g. function keys)', () => {
    expect(
      eventToShortcut(new KeyboardEvent('keydown', { code: 'F1' })),
    ).toBeNull()
  })
})

describe('useShortcutCapture — hook', () => {
  let onCapture: ReturnType<typeof vi.fn>
  beforeEach(() => { onCapture = vi.fn() })

  it('delivers the captured combo when active', () => {
    renderHook(() => useShortcutCapture({ active: true, onCapture }))
    fireKey({ code: 'KeyP', metaKey: true, shiftKey: true })
    expect(onCapture).toHaveBeenCalledWith('Cmd+Shift+P')
  })

  it('does NOT capture when inactive', () => {
    renderHook(() => useShortcutCapture({ active: false, onCapture }))
    fireKey({ code: 'KeyP', metaKey: true, shiftKey: true })
    expect(onCapture).not.toHaveBeenCalled()
  })

  it('delivers null on Escape (cancel)', () => {
    renderHook(() => useShortcutCapture({ active: true, onCapture }))
    fireKey({ code: 'Escape' })
    expect(onCapture).toHaveBeenCalledWith(null)
  })

  it('ignores modifier-only presses, waits for a real key', () => {
    renderHook(() => useShortcutCapture({ active: true, onCapture }))
    fireKey({ code: 'MetaLeft', metaKey: true })
    expect(onCapture).not.toHaveBeenCalled()
    fireKey({ code: 'KeyA', metaKey: true })
    expect(onCapture).toHaveBeenCalledWith('Cmd+A')
  })

  it('does not double-fire when re-rendered with the same active state', () => {
    const { rerender } = renderHook(
      ({ a }) => useShortcutCapture({ active: a, onCapture }),
      { initialProps: { a: true } },
    )
    rerender({ a: true })
    fireKey({ code: 'KeyB', metaKey: true })
    expect(onCapture).toHaveBeenCalledTimes(1)
  })
})

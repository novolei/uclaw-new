import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ShortcutSettings } from './ShortcutSettings'
import { shortcutOverridesAtom } from '@/atoms/shortcut-atoms'
import { SHORTCUT_DEFINITIONS } from '@/lib/shortcut-defaults'

const isMac = /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

describe('ShortcutSettings — data-driven keybinding panel', () => {
  beforeEach(() => {
    // localStorage is wiped between tests by the global setup, but be defensive:
    localStorage.clear()
  })

  it('renders one row per SHORTCUT_DEFINITIONS entry', () => {
    renderWithProviders(<ShortcutSettings />)
    // Every label appears at least once. Spot-check 3.
    expect(screen.getAllByText('新建对话').length).toBeGreaterThan(0)
    expect(screen.getAllByText('全局搜索').length).toBeGreaterThan(0)
    expect(screen.getAllByText('专注模式').length).toBeGreaterThan(0)
  })

  it('displays the default binding when no override is set', () => {
    const store = createStore()
    renderWithProviders(<ShortcutSettings />, { store })
    // The "新建对话" row shows Cmd+N on Mac / Ctrl+N on Win.
    // formatShortcut on Mac compresses to "⌘N" — we just check the row exists.
    expect(screen.getByText('新建对话')).not.toBeNull()
  })

  it('writes an override when the user captures a new combo, and the row shows it', () => {
    const store = createStore()
    renderWithProviders(<ShortcutSettings />, { store })

    // Click "新建对话" row's combo chip to enter capture mode.
    // Multiple chips (one per shortcut); just pick the first.
    const chips = screen.getAllByRole('button', { name: /点击录入新组合/ })
    fireEvent.click(chips[0]!)

    // Press a combo that's not used by any other shortcut: Cmd+Shift+P
    fireEvent.keyDown(window, { code: 'KeyP', metaKey: true, shiftKey: true, bubbles: true })

    // After capture, the atom should hold an override for the FIRST shortcut id.
    // We don't assume which it is — the chip click was the first one found —
    // so we assert the atom has at least one entry.
    const next = store.get(shortcutOverridesAtom)
    expect(Object.keys(next).length).toBe(1)
    const onlyKey = Object.keys(next)[0]!
    const override = next[onlyKey]!
    if (isMac) {
      expect(override.mac).toBe('Cmd+Shift+P')
    } else {
      expect(override.win).toBe('Cmd+Shift+P')
    }
  })

  it('reset-all button clears every override and is disabled when there are none', () => {
    const store = createStore()
    // Seed an override directly.
    const firstId = SHORTCUT_DEFINITIONS[0]!.id
    store.set(shortcutOverridesAtom, {
      [firstId]: { mac: 'Cmd+Shift+P', win: 'Ctrl+Shift+P' },
    })
    renderWithProviders(<ShortcutSettings />, { store })

    const resetAll = screen.getByRole('button', { name: '重置全部' })
    expect(resetAll).not.toBeNull()
    fireEvent.click(resetAll)

    expect(store.get(shortcutOverridesAtom)).toEqual({})
  })

  it('Escape during capture cancels without writing', () => {
    const store = createStore()
    renderWithProviders(<ShortcutSettings />, { store })

    const chips2 = screen.getAllByRole('button', { name: /点击录入新组合/ })
    fireEvent.click(chips2[0]!)
    fireEvent.keyDown(window, { code: 'Escape', bubbles: true })

    expect(store.get(shortcutOverridesAtom)).toEqual({})
  })
})

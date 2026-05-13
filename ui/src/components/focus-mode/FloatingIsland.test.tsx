import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FloatingIsland } from './FloatingIsland'
import {
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'

describe('FloatingIsland', () => {
  it('renders children when reveal matches its side', () => {
    const store = createStore()
    store.set(focusRevealSideAtom, 'left')
    renderWithProviders(
      <FloatingIsland side="left">
        <div>left-children</div>
      </FloatingIsland>,
      { store },
    )
    expect(screen.queryByText('left-children')).not.toBeNull()
  })

  it('does NOT render children when reveal is the other side', () => {
    const store = createStore()
    store.set(focusRevealSideAtom, 'right')
    renderWithProviders(
      <FloatingIsland side="left">
        <div>left-children</div>
      </FloatingIsland>,
      { store },
    )
    expect(screen.queryByText('left-children')).toBeNull()
  })

  it('clicking inside the island does NOT auto-pin (2026-05-13 fix)', async () => {
    // Previous behaviour: clicking any element inside the island set
    // focusRevealPinnedAtom = true, which then froze the hotzone leave
    // timer and the island stuck on screen until clicked outside.
    // New behaviour: plain clicks do not pin. Hotzone's 200ms leave timer
    // fires the moment the mouse exits the island region — the user can
    // click a session row and then drift the mouse back to the preview
    // and the island will auto-hide.
    const store = createStore()
    store.set(focusRevealSideAtom, 'left')
    const { user } = renderWithProviders(
      <FloatingIsland side="left">
        <button>inside-button</button>
      </FloatingIsland>,
      { store },
    )
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
    await user.click(screen.getByText('inside-button'))
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })
})

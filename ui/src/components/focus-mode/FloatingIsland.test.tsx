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

  it('clicking inside the island sets pinned = true', async () => {
    const store = createStore()
    store.set(focusRevealSideAtom, 'left')
    const { user } = renderWithProviders(
      <FloatingIsland side="left">
        <button>inside-button</button>
      </FloatingIsland>,
      { store },
    )
    await user.click(screen.getByText('inside-button'))
    expect(store.get(focusRevealPinnedAtom)).toBe(true)
  })
})

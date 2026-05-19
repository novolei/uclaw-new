import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { DockPinnedItem } from './DockPinnedItem'

vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style, ...rest }, ref) =>
      React.createElement('button', { ref, style: style as React.CSSProperties, ...rest }),
    ),
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
}))

describe('DockPinnedItem', () => {
  it('renders the first character of the label, uppercased', () => {
    render(
      <DockPinnedItem
        sortableId="space-w1"
        label="research"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByRole('button', { name: 'research' })).toBeInTheDocument()
    expect(screen.getByText('R')).toBeInTheDocument()
  })

  it('renders the explicit emoji when provided (takes precedence over label initial)', () => {
    render(
      <DockPinnedItem
        sortableId="space-w2"
        label="research"
        emoji="🧪"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByText('🧪')).toBeInTheDocument()
    expect(screen.queryByText('R')).toBeNull()
  })

  it('renders an empty fallback when label is empty', () => {
    const { container } = render(
      <DockPinnedItem
        sortableId="space-w3"
        label=""
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const btn = screen.getByRole('button', { name: '' })
    expect(btn).toBeInTheDocument()
    const glyph = container.querySelector('[data-dock-pin-glyph]') as HTMLElement
    expect(glyph?.textContent).toBe('')
  })

  it('reflects sortable id in data-sortable-id', () => {
    render(
      <DockPinnedItem
        sortableId="conv-sess-42"
        label="Old chat about onboarding"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(
      screen.getByRole('button', { name: 'Old chat about onboarding' }).getAttribute('data-sortable-id'),
    ).toBe('conv-sess-42')
  })

  it('applies the deterministic color gradient as background', () => {
    const { container } = render(
      <DockPinnedItem
        sortableId="space-w1"
        label="research"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const tile = container.querySelector('[data-dock-pin-tile]') as HTMLElement
    expect(tile.style.background).toMatch(/linear-gradient/)
  })

  it('wraps the button in a ContextMenu (trigger has data-state="closed")', () => {
    const { container } = render(
      <DockPinnedItem
        sortableId="space-w1"
        label="research"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    // Radix ContextMenuTrigger marks itself with data-state="closed" when idle.
    const trigger = container.querySelector('[data-state="closed"]')
    expect(trigger).not.toBeNull()
  })
})

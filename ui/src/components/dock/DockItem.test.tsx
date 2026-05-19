import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import React from 'react'
import { DockItem } from './DockItem'
import { Bot } from 'lucide-react'

// motion/react is mocked so springs resolve synchronously and DOM stays plain.
// motion.button is forwardRef-wrapped so Radix Tooltip's `asChild` Slot can
// forward its ref without React warnings.
vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style, ...rest }, ref) =>
      React.createElement('button', { ref, style, ...rest }),
    ),
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
}))

describe('DockItem', () => {
  it('exposes label via aria-label (not inline text)', () => {
    // Labels migrated from always-visible inline span → hover Tooltip.
    // The accessibility name still comes from aria-label so screen
    // readers and tests can find the button by name.
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByRole('button', { name: 'Agent' })).toBeInTheDocument()
  })

  it('reflects active state via aria-pressed', () => {
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByRole('button', { name: 'Agent' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })

  it('calls onClick when clicked', () => {
    const onClick = vi.fn()
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={onClick}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('notifies hover index on enter/leave', () => {
    const onHoverIndexChange = vi.fn()
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={2}
        hoveredIndex={null}
        onHoverIndexChange={onHoverIndexChange}
        onClick={vi.fn()}
      />,
    )
    const btn = screen.getByRole('button', { name: 'Agent' })
    fireEvent.mouseEnter(btn)
    expect(onHoverIndexChange).toHaveBeenLastCalledWith(2)
    fireEvent.mouseLeave(btn)
    expect(onHoverIndexChange).toHaveBeenLastCalledWith(null)
  })

  it('does NOT render a colored slot backplate around the icon', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const button = screen.getByRole('button', { name: 'Agent' })
    const html = button.outerHTML
    expect(html).not.toMatch(/bg-primary\/12/)
    expect(html).not.toMatch(/bg-foreground\/\[0\.06\]/)
    expect(html).not.toMatch(/ring-primary\/30/)
  })

  it('renders the active-state dot only when isActive', () => {
    const { container: activeContainer } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(activeContainer.querySelector('[data-dock-active-dot]')).not.toBeNull()

    const { container: inactiveContainer } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={1}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(inactiveContainer.querySelector('[data-dock-active-dot]')).toBeNull()
  })

  it('renders a 56 px outer slot with a 44 px inner icon box', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const btn = screen.getByRole('button', { name: 'Agent' })
    // Outer slot (SLOT_W = 56)
    expect(btn.style.width).toBe('56px')
    expect(btn.style.height).toBe('56px')
    // Inner icon-box span (ICON_BOX = 44). It's the only <span> child of the
    // button at this point (the active-dot span only renders when isActive).
    const iconBox = container.querySelector('button > span') as HTMLElement
    expect(iconBox.style.width).toBe('44px')
    expect(iconBox.style.height).toBe('44px')
  })
})

import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import React from 'react'
import { DockItem } from './DockItem'
import { Bot } from 'lucide-react'

vi.mock('motion/react', () => ({
  motion: {
    button: ({ style: _style, ...rest }: React.ComponentPropsWithoutRef<'button'>) =>
      React.createElement('button', rest),
  },
  useSpring: () => ({ set: vi.fn() }),
}))

describe('DockItem', () => {
  it('renders the label when active', () => {
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={true}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />
    )
    const label = screen.getByText('Agent')
    expect(label).toBeInTheDocument()
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
      />
    )
    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(onClick).toHaveBeenCalledOnce()
  })
})

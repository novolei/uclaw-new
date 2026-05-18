import * as React from 'react'
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Bot } from 'lucide-react'

vi.mock('motion/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('motion/react')>()
  return {
    ...actual,
    motion: {
      ...actual.motion,
      button: (props: React.ComponentPropsWithoutRef<'button'>) =>
        React.createElement('button', props),
    },
    useSpring: () => ({
      set: vi.fn(),
    }),
  }
})

import { DockItem } from './DockItem'

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

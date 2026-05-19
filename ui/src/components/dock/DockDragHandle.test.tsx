import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { DockDragHandle } from './DockDragHandle'

describe('DockDragHandle', () => {
  it('renders 4 decorative dots inside a hidden wrapper', () => {
    const { container } = render(<DockDragHandle />)
    const handle = container.querySelector('[data-dock-drag-handle]') as HTMLElement
    expect(handle).not.toBeNull()
    expect(handle.getAttribute('aria-hidden')).toBe('true')
    const dots = handle.querySelectorAll('span')
    expect(dots.length).toBe(4)
  })

  it('is in idle state by default (data-state="idle")', () => {
    const { container } = render(<DockDragHandle />)
    const handle = container.querySelector('[data-dock-drag-handle]') as HTMLElement
    expect(handle.getAttribute('data-state')).toBe('idle')
  })
})

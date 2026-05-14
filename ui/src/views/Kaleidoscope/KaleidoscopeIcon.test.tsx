import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { KaleidoscopeIcon } from './KaleidoscopeIcon'

describe('KaleidoscopeIcon', () => {
  it('renders a button with the accessible label', () => {
    render(<KaleidoscopeIcon />)
    expect(screen.getByRole('button', { name: '打开万花筒' })).toBeInTheDocument()
  })

  it('fires onClick when clicked', async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(<KaleidoscopeIcon onClick={onClick} />)
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('shows a solid background only when active', () => {
    const { rerender } = render(<KaleidoscopeIcon active={false} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    // idle: no standalone bg, only a `hover:bg-primary/10` variant.
    // active: a solid `bg-primary/20`. Assert on /20 — it exists only in the
    // active state (idle has no /20 at all).
    expect(btn.className).not.toMatch(/bg-primary\/20/)
    rerender(<KaleidoscopeIcon active />)
    expect(btn.className).toMatch(/bg-primary\/20/)
  })

  it('bursts confetti on hover', async () => {
    const user = userEvent.setup()
    render(<KaleidoscopeIcon />)
    expect(screen.queryByTestId('kaleidoscope-confetti')).not.toBeInTheDocument()
    await user.hover(screen.getByRole('button', { name: '打开万花筒' }))
    expect(screen.getByTestId('kaleidoscope-confetti')).toBeInTheDocument()
  })
})

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

  it('applies the idle background tint only when not active', () => {
    const { rerender } = render(<KaleidoscopeIcon active={false} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    // idle: bg-primary/10 ; active: bg-primary/20. Assert on bg-primary/10 —
    // it appears only in the idle state (active has no /10), whereas
    // bg-primary/20 also lives in the idle `hover:` variant so it can't
    // distinguish the two.
    expect(btn.className).toMatch(/bg-primary\/10/)
    rerender(<KaleidoscopeIcon active />)
    expect(btn.className).not.toMatch(/bg-primary\/10/)
  })

  it('bursts confetti on click', async () => {
    const user = userEvent.setup()
    render(<KaleidoscopeIcon />)
    expect(screen.queryByTestId('kaleidoscope-confetti')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(screen.getByTestId('kaleidoscope-confetti')).toBeInTheDocument()
  })
})

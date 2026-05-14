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

  it('applies the active ring only when active', () => {
    const { rerender } = render(<KaleidoscopeIcon active={false} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    expect(btn.className).not.toMatch(/ring-2/)
    rerender(<KaleidoscopeIcon active />)
    expect(btn.className).toMatch(/ring-2/)
  })

  it('honours the size prop', () => {
    render(<KaleidoscopeIcon size={48} />)
    const btn = screen.getByRole('button', { name: '打开万花筒' })
    expect(btn.style.width).toBe('48px')
    expect(btn.style.height).toBe('48px')
  })
})

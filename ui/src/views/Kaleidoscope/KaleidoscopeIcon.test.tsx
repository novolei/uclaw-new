import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { KaleidoscopeIcon } from './KaleidoscopeIcon'

// Mock lottie-react so the test doesn't need a real canvas/animation runtime.
vi.mock('lottie-react', () => ({
  default: () => <div data-testid="lottie-stub" />,
}))

describe('KaleidoscopeIcon', () => {
  it('renders the static fallback when no animationData is provided', () => {
    render(<KaleidoscopeIcon />)
    expect(screen.getByLabelText('万花筒')).toBeInTheDocument()
    expect(screen.queryByTestId('lottie-stub')).not.toBeInTheDocument()
  })

  it('renders the Lottie player when animationData is provided', () => {
    render(<KaleidoscopeIcon animationData={{ v: '5.0.0', fr: 30, layers: [] }} />)
    expect(screen.getByTestId('lottie-stub')).toBeInTheDocument()
  })

  it('is a button with an accessible label and fires onClick', async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(<KaleidoscopeIcon onClick={onClick} />)
    await user.click(screen.getByRole('button', { name: '打开万花筒' }))
    expect(onClick).toHaveBeenCalledOnce()
  })
})

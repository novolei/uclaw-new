import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { KaleidoscopeIconFallback } from './KaleidoscopeIconFallback'

describe('KaleidoscopeIconFallback', () => {
  it('renders an svg with the kaleidoscope aria-label', () => {
    render(<KaleidoscopeIconFallback />)
    const el = screen.getByLabelText('万花筒')
    expect(el).toBeInTheDocument()
  })

  it('honours the size prop', () => {
    const { container } = render(<KaleidoscopeIconFallback size={48} />)
    const wrapper = container.firstElementChild as HTMLElement
    expect(wrapper.style.width).toBe('48px')
    expect(wrapper.style.height).toBe('48px')
  })
})

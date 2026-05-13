import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { GlowIndicator } from './GlowIndicator'

describe('GlowIndicator', () => {
  it('renders the three-layer glow + trace with the correct side classes', () => {
    renderWithProviders(<GlowIndicator side="left" />)
    const wrapper = screen.getByTestId('focus-glow-left')
    expect(wrapper).not.toBeNull()
    // Each of halo / soft / core / trace should be present
    expect(wrapper.querySelector('.focus-glow-halo')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-soft')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-core')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-trace')).not.toBeNull()
  })
})

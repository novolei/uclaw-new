import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { MemoryModule } from './MemoryModule'

vi.mock('@/components/memory/MemoryGraphView', () => ({
  MemoryGraphView: () => <div data-testid="memory-graph-view" />,
}))

describe('MemoryModule', () => {
  it('renders MemoryGraphView full-bleed', () => {
    const { container } = renderWithProviders(<MemoryModule />)
    expect(screen.getByTestId('memory-graph-view')).toBeInTheDocument()
    // full-bleed wrapper —— absolute inset-0,与 HumansModule 同款
    expect(container.querySelector('.absolute.inset-0')).not.toBeNull()
  })
})

import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { IntelligenceTab } from './IntelligenceTab'

describe('IntelligenceTab', () => {
  it('renders without throwing and contains data-settings-section markers', () => {
    const { container } = renderWithProviders(<IntelligenceTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(3)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('模型分配')
    expect(names).toContain('Agent 行为')
    expect(names).toContain('提示词')
  })
})

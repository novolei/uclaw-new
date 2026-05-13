import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ToolsTab } from './ToolsTab'

describe('ToolsTab', () => {
  it('renders 3 sub-section markers', () => {
    const { container } = renderWithProviders(<ToolsTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(3)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('工具与 MCP')
    expect(names).toContain('工具权限')
    expect(names).toContain('已学技能')
  })
})

import { describe, test, expect } from 'vitest'
import { render } from '@testing-library/react'
import { AppTypeBadge } from './AppTypeBadge'

describe('AppTypeBadge', () => {
  test('renders Chinese label for automation', () => {
    const { getByText } = render(<AppTypeBadge type="automation" />)
    expect(getByText('数字人')).toBeInTheDocument()
  })
  test('renders MCP label', () => {
    const { getByText } = render(<AppTypeBadge type="mcp" />)
    expect(getByText('MCP')).toBeInTheDocument()
  })
  test('falls back to raw type string for unknown type', () => {
    const { getByText } = render(<AppTypeBadge type="exotic-type" />)
    expect(getByText('exotic-type')).toBeInTheDocument()
  })
  test('exposes tooltip via title attribute', () => {
    const { container } = render(<AppTypeBadge type="automation" />)
    const el = container.querySelector('[title]')
    expect(el?.getAttribute('title')).toContain('自动化数字员工')
  })
})

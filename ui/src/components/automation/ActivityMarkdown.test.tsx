import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { ActivityMarkdown } from './ActivityMarkdown'

describe('ActivityMarkdown', () => {
  it('renders bold text', () => {
    const { container } = render(<ActivityMarkdown content="**bold**" />)
    expect(container.querySelector('strong')).toBeTruthy()
  })

  it('renders inline code', () => {
    const { container } = render(<ActivityMarkdown content="`code`" />)
    expect(container.querySelector('code')).toBeTruthy()
  })

  it('renders a GFM table', () => {
    const { container } = render(
      <ActivityMarkdown content={'| A | B |\n|---|---|\n| 1 | 2 |'} />
    )
    expect(container.querySelector('table')).toBeTruthy()
  })

  it('accepts an extra className', () => {
    const { container } = render(
      <ActivityMarkdown content="hi" className="custom-class" />
    )
    expect(container.firstElementChild?.className).toContain('custom-class')
  })
})

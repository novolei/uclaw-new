import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ModuleHeader } from './ModuleHeader'

describe('ModuleHeader', () => {
  it('renders group label, title and subtitle', () => {
    render(
      <ModuleHeader group="asset" title="数字人" subtitle="5 个 · 本周 18 次执行" />,
    )
    expect(screen.getByText('资产')).toBeInTheDocument()
    expect(screen.getByText('数字人')).toBeInTheDocument()
    expect(screen.getByText('5 个 · 本周 18 次执行')).toBeInTheDocument()
  })

  it('renders the capability group label', () => {
    render(<ModuleHeader group="capability" title="技能" />)
    expect(screen.getByText('能力')).toBeInTheDocument()
  })

  it('renders action node when provided', () => {
    render(
      <ModuleHeader
        group="asset"
        title="数字人"
        actions={<button type="button">+ 新建</button>}
      />,
    )
    expect(screen.getByRole('button', { name: '+ 新建' })).toBeInTheDocument()
  })
})

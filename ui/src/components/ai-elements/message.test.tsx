import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { MessageResponse } from './message'

describe('MessageResponse — headings', () => {
  it('renders h2 with accent bar wrapper', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'## Project Overview'}</MessageResponse>,
    )
    const h2 = container.querySelector('h2')
    expect(h2).not.toBeNull()
    expect(h2!.textContent).toContain('Project Overview')
    expect(h2!.classList.toString()).toContain('flex')
    const accentBar = h2!.querySelector('span[aria-hidden]')
    expect(accentBar).not.toBeNull()
  })

  it('renders h1 with accent bar wrapper', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'# Top Title'}</MessageResponse>,
    )
    const h1 = container.querySelector('h1')
    expect(h1).not.toBeNull()
    expect(h1!.textContent).toContain('Top Title')
    expect(h1!.querySelector('span[aria-hidden]')).not.toBeNull()
  })

  it('renders h3 without accent bar', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{'### Subhead'}</MessageResponse>,
    )
    const h3 = container.querySelector('h3')
    expect(h3).not.toBeNull()
    expect(h3!.textContent).toContain('Subhead')
    expect(h3!.querySelector('span[aria-hidden]')).toBeNull()
  })
})

describe('MessageResponse — tables', () => {
  const tableMd = [
    '| Project | Status |',
    '|---------|--------|',
    '| Alpha   | done   |',
    '| Beta    | wip    |',
  ].join('\n')

  it('wraps table in a card container', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const table = container.querySelector('table')
    expect(table).not.toBeNull()
    const wrapper = table!.parentElement
    expect(wrapper).not.toBeNull()
    expect(wrapper!.className).toContain('rounded-')
    expect(wrapper!.className).toContain('border')
  })

  it('renders thead with muted background', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const thead = container.querySelector('thead')
    expect(thead).not.toBeNull()
    expect(thead!.className).toContain('bg-muted')
  })

  it('renders th with uppercase + tracking + muted-foreground', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableMd}</MessageResponse>,
    )
    const th = container.querySelector('th')
    expect(th).not.toBeNull()
    expect(th!.className).toContain('uppercase')
    expect(th!.className).toContain('tracking-')
    expect(th!.className).toContain('text-muted-foreground')
  })
})

describe('MessageResponse — status badges in table cells', () => {
  function tableWithStatus(status: string): string {
    return [
      '| Project | Status |',
      '|---------|--------|',
      `| Alpha   | ${status} |`,
    ].join('\n')
  }

  it('detects success badge for "✅ 已完成"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('✅ 已完成')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const statusCell = cells[1]!
    const badge = statusCell.querySelector('span[data-status]')
    expect(badge).not.toBeNull()
    expect(badge!.getAttribute('data-status')).toBe('success')
  })

  it('detects warning badge for "⏳ 未完成"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('⏳ 未完成')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const badge = cells[1]!.querySelector('span[data-status="warning"]')
    expect(badge).not.toBeNull()
  })

  it('detects danger badge for "❌ 尚未开始"', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('❌ 尚未开始')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const badge = cells[1]!.querySelector('span[data-status="danger"]')
    expect(badge).not.toBeNull()
  })

  it('renders cell content unchanged when no status pattern matches', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('HTML/CSS/JS')}</MessageResponse>,
    )
    const cells = Array.from(container.querySelectorAll('td'))
    const statusCell = cells[1]!
    expect(statusCell.querySelector('span[data-status]')).toBeNull()
    expect(statusCell.textContent).toBe('HTML/CSS/JS')
  })

  it('badge uses success token classes', () => {
    const { container } = renderWithProviders(
      <MessageResponse>{tableWithStatus('✅ done')}</MessageResponse>,
    )
    const badge = container.querySelector('span[data-status="success"]')!
    expect(badge.className).toContain('hsl(var(--success-bg))')
    expect(badge.className).toContain('hsl(var(--success))')
  })
})

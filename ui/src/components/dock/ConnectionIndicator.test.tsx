import * as React from 'react'
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { ConnectionIndicator } from './ConnectionIndicator'
import { internetOnlineAtom, backendOnlineAtom } from '@/atoms/dock-atoms'

function renderWithStore(overrides: { internet?: boolean; backend?: boolean } = {}) {
  const store = createStore()
  if (overrides.internet !== undefined) store.set(internetOnlineAtom, overrides.internet)
  if (overrides.backend !== undefined) store.set(backendOnlineAtom, overrides.backend)
  return render(
    <JotaiProvider store={store}>
      <ConnectionIndicator />
    </JotaiProvider>,
  )
}

describe('ConnectionIndicator', () => {
  it('renders the status container with aria-label', () => {
    renderWithStore({ internet: true, backend: true })
    expect(screen.getByLabelText('连接状态')).toBeInTheDocument()
  })

  it('renders exactly 2 signal bars', () => {
    const { container } = renderWithStore({ internet: true, backend: true })
    const bars = container.querySelectorAll('[data-conn-bar]')
    expect(bars.length).toBe(2)
  })

  it('marks an offline channel with data-state=offline', () => {
    const { container } = renderWithStore({ internet: false, backend: true })
    const internetBar = container.querySelector('[data-conn-bar="internet"]')
    expect(internetBar?.getAttribute('data-state')).toBe('offline')
    // When internet is offline, backend cascades to offline too.
    expect(container.querySelector('[data-conn-bar="backend"]')?.getAttribute('data-state')).toBe('offline')
  })

  it('unified container is the tooltip trigger — no individual aria-label on bars', () => {
    const { container } = renderWithStore({ internet: true, backend: true })
    // Individual bars are visual-only; the group container carries the a11y surface.
    const bars = container.querySelectorAll('[data-conn-bar]')
    bars.forEach((bar) => {
      expect(bar.getAttribute('aria-label')).toBeNull()
      expect(bar.getAttribute('role')).toBeNull()
    })
  })
})

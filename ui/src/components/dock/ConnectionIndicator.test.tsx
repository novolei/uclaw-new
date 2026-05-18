import * as React from 'react'
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { createStore } from 'jotai'
import { Provider as JotaiProvider } from 'jotai'
import { ConnectionIndicator } from './ConnectionIndicator'
import { internetOnlineAtom, backendOnlineAtom, memuOnlineAtom } from '@/atoms/dock-atoms'

function renderWithStore(overrides: { internet?: boolean; backend?: boolean; memu?: boolean | null } = {}) {
  const store = createStore()
  if (overrides.internet !== undefined) store.set(internetOnlineAtom, overrides.internet)
  if (overrides.backend !== undefined) store.set(backendOnlineAtom, overrides.backend)
  if (overrides.memu !== undefined) store.set(memuOnlineAtom, overrides.memu)
  return render(
    <JotaiProvider store={store}>
      <ConnectionIndicator />
    </JotaiProvider>
  )
}

describe('ConnectionIndicator', () => {
  it('renders the status container', () => {
    renderWithStore({ internet: true, backend: true, memu: true })
    expect(screen.getByLabelText('连接状态')).toBeInTheDocument()
  })

  it('renders three dots', () => {
    const { container } = renderWithStore({ internet: true, backend: true, memu: true })
    const dots = container.querySelectorAll('[class*="rounded-full"]')
    expect(dots.length).toBeGreaterThanOrEqual(3)
  })
})

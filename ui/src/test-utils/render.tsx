/**
 * renderWithProviders — wraps RTL's render() with the providers most
 * uClaw components expect (Jotai store, Radix Tooltip provider).
 *
 * Usage:
 *   const { user } = renderWithProviders(<MyComponent />)
 *   await user.click(screen.getByRole('button'))
 */

import * as React from 'react'
import { render, type RenderOptions, type RenderResult } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'

type JotaiStore = ReturnType<typeof createStore>

export interface ProviderOptions extends Omit<RenderOptions, 'wrapper'> {
  /** Optional pre-seeded Jotai store. Default: a fresh empty store. */
  store?: JotaiStore
}

export interface ProviderRenderResult extends RenderResult {
  /** UserEvent instance scoped to this render call. */
  user: ReturnType<typeof userEvent.setup>
  /** The Jotai store used by this render — useful for asserting atom values. */
  store: JotaiStore
}

export function renderWithProviders(
  ui: React.ReactElement,
  options: ProviderOptions = {},
): ProviderRenderResult {
  const { store = createStore(), ...rtlOptions } = options
  const Wrapper = ({ children }: { children: React.ReactNode }) => (
    <JotaiProvider store={store}>
      <TooltipProvider>{children}</TooltipProvider>
    </JotaiProvider>
  )
  const result = render(ui, { wrapper: Wrapper, ...rtlOptions })
  const user = userEvent.setup()
  return { ...result, user, store }
}

// Re-export RTL's screen / waitFor / fireEvent so tests have a single import surface.
export { screen, waitFor, fireEvent, within } from '@testing-library/react'

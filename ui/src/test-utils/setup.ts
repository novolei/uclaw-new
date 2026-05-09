/**
 * Vitest global setup — runs once before any test file.
 *
 * - Imports @testing-library/jest-dom matchers so `expect(el).toBeInTheDocument()` works.
 * - Cleans up the DOM after each test (RTL does this automatically when @testing-library/react
 *   detects vitest, but we wire it explicitly to avoid surprises).
 * - Stubs `matchMedia` since jsdom doesn't implement it but `useTheme` / a few atoms read it.
 */

import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'

afterEach(() => {
  cleanup()
})

// jsdom doesn't implement matchMedia; stub it so theme / responsive code doesn't crash.
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},        // deprecated but some libs still call it
    removeListener: () => {},     // same
    dispatchEvent: () => false,
  }),
})

// jsdom doesn't have ResizeObserver — Conversation uses it in P4 area, but other components
// might too. Stub with a no-op so tests don't crash if a component initializes one.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
;(window as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
  ResizeObserverStub

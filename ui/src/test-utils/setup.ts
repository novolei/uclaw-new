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
// Also: report `prefers-reduced-motion: reduce` so the `motion` library
// skips animations globally in tests — without this, AnimatePresence's
// mode="wait" defers re-renders past synchronous test assertions and
// every workspace-switch / dialog test goes flaky.
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: query.includes('prefers-reduced-motion') && query.includes('reduce'),
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},        // deprecated but some libs still call it
    removeListener: () => {},     // same
    dispatchEvent: () => false,
  }),
})

// jsdom on Node 22 ships a `window.localStorage` placeholder without `getItem` /
// `setItem` etc. — atomWithStorage (used by appModeAtom and friends) crashes on
// mount with `getItem is not a function`. Replace with a minimal in-memory shim.
{
  const store = new Map<string, string>()
  const storageStub: Storage = {
    get length() { return store.size },
    clear: () => store.clear(),
    getItem: (k) => (store.has(k) ? store.get(k)! : null),
    key: (i) => Array.from(store.keys())[i] ?? null,
    removeItem: (k) => { store.delete(k) },
    setItem: (k, v) => { store.set(k, String(v)) },
  }
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: storageStub,
  })
  Object.defineProperty(window, 'sessionStorage', {
    configurable: true,
    value: storageStub,
  })
}

// jsdom doesn't have ResizeObserver — Conversation uses it in P4 area, but other components
// might too. Stub with a no-op so tests don't crash if a component initializes one.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
;(window as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
  ResizeObserverStub


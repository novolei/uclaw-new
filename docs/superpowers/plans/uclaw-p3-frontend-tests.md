# P3 — Frontend Test Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up Vitest + React Testing Library, write a small starter test suite covering the chat layer, and wire it into npm scripts. This is the prerequisite for P4 / P5 / P6 / P11 — every UI plan after this should add tests.

**Architecture:** Vitest extends the existing Vite config (no duplicate build pipeline). `jsdom` env for DOM. A small `test-utils/` directory with a `renderWithProviders` helper (Jotai store + Tooltip provider) and `mock-tauri` helpers (vi.mock wrappers for `invoke` / `listen`). Four starter test files cover the components and hooks that PR #21 + #24 just touched and that have non-trivial logic worth locking in.

**Tech Stack:** Vitest, @testing-library/react, @testing-library/jest-dom, @testing-library/user-event, jsdom — all pure devDependencies. No runtime impact.

**Reference roadmap:** `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md` §P3.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p3-frontend-tests
```

- [ ] **Step 0.2: Sanity check that the baseline frontend still builds**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -3
npx vite build 2>&1 | tail -3
```
Expected: zero TS errors, build succeeds. (Recent merges touched several files — confirm we're starting from a green baseline.)

---

## Task 1: Install dev dependencies + Vitest config

**Files:**
- Modify: `ui/package.json` (devDependencies + scripts)
- Create: `ui/vitest.config.ts`
- Modify: `.gitignore` (add `coverage/` if not present)

- [ ] **Step 1.1: Install the test stack**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm install --save-dev \
  vitest@^1.6.0 \
  @vitest/ui@^1.6.0 \
  @testing-library/react@^16.0.0 \
  @testing-library/jest-dom@^6.5.0 \
  @testing-library/user-event@^14.5.2 \
  jsdom@^25.0.0
```

Versions pinned to a known-compatible set (Vitest 1.6 + RTL 16 + React 18). If npm warns about peer deps, those are resolvable.

- [ ] **Step 1.2: Add scripts to `ui/package.json`**

Edit the `"scripts"` object (currently has `dev` / `build` / `preview`). Add:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:ui": "vitest --ui",
    "test:coverage": "vitest run --coverage"
  }
}
```

- [ ] **Step 1.3: Create `ui/vitest.config.ts`**

```ts
import { defineConfig, mergeConfig } from 'vitest/config'
import viteConfig from './vite.config'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      globals: true,
      setupFiles: ['./src/test-utils/setup.ts'],
      include: ['src/**/*.{test,spec}.{ts,tsx}'],
      exclude: ['node_modules', 'src-tauri', '../static'],
      // Run tests in parallel for speed; fail fast on first failure during dev.
      pool: 'threads',
      // Helpful default reporter
      reporters: ['default'],
      coverage: {
        provider: 'v8',
        reporter: ['text', 'html'],
        include: ['src/**/*.{ts,tsx}'],
        exclude: [
          'src/**/*.test.{ts,tsx}',
          'src/**/*.spec.{ts,tsx}',
          'src/test-utils/**',
          'src/main.tsx',
        ],
      },
    },
  }),
)
```

`mergeConfig(viteConfig, ...)` ensures the `@/` alias from `vite.config.ts` resolves the same way in tests.

- [ ] **Step 1.4: Create `ui/src/test-utils/setup.ts`**

```ts
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
```

- [ ] **Step 1.5: Update `.gitignore`**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -q "^coverage/" ui/.gitignore 2>/dev/null || echo "coverage/" >> ui/.gitignore
grep -q "^.vitest/" ui/.gitignore 2>/dev/null || echo ".vitest/" >> ui/.gitignore
```

(If `ui/.gitignore` doesn't exist, the repo-root `.gitignore` is used — append there instead.)

- [ ] **Step 1.6: Run a smoke check that vitest at least starts**

```bash
cd ui && npx vitest run 2>&1 | tail -10
```
Expected: vitest exits 0 with "No test files found" OR "0 passed" — no test files yet, but the runner must initialize without errors. If it errors on config or setup, fix before continuing.

- [ ] **Step 1.7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/package.json ui/package-lock.json ui/vitest.config.ts ui/src/test-utils/setup.ts ui/.gitignore .gitignore
git status --short
git commit -m "$(cat <<'EOF'
chore(test): set up Vitest + React Testing Library

Per roadmap P3 — frontend has had zero tests since the project started.
Stand up the harness before writing any tests so future UI plans can
add coverage without re-inventing the setup.

Stack:
  - vitest@^1.6 + @vitest/ui — test runner, parallel by default
  - @testing-library/react@^16, @testing-library/jest-dom,
    @testing-library/user-event — component test ergonomics
  - jsdom@^25 — DOM env for non-browser tests

Configuration:
  - ui/vitest.config.ts extends vite.config.ts via mergeConfig so
    the @/ alias resolves identically in tests
  - ui/src/test-utils/setup.ts wires jest-dom matchers, RTL cleanup,
    matchMedia + ResizeObserver stubs (jsdom misses both)
  - npm scripts: test (CI), test:watch (dev), test:ui (browser
    dashboard), test:coverage

Coverage output dir gitignored. No tests yet — those land in
subsequent commits.
EOF
)"
```

---

## Task 2: Test utilities — `renderWithProviders` + `mock-tauri`

**Files:**
- Create: `ui/src/test-utils/render.tsx`
- Create: `ui/src/test-utils/mock-tauri.ts`

These shave boilerplate from every component test. Without them each test would re-instantiate Jotai stores and stub Tauri APIs by hand.

- [ ] **Step 2.1: Create `ui/src/test-utils/render.tsx`**

```tsx
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
```

- [ ] **Step 2.2: Create `ui/src/test-utils/mock-tauri.ts`**

```ts
/**
 * Mock helpers for @tauri-apps/api so component tests don't need a running
 * Tauri runtime. Only the surface the chat layer actually uses.
 *
 * Apply at the top of a test file:
 *
 *   import { vi } from 'vitest'
 *   import { mockInvoke, mockListen } from '@/test-utils/mock-tauri'
 *
 *   vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }))
 *   vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }))
 *
 *   beforeEach(() => { mockInvoke.mockClear(); mockListen.mockClear() })
 *
 * Stub specific commands per test:
 *
 *   mockInvoke.mockImplementation((cmd, args) => {
 *     if (cmd === 'get_messages') return Promise.resolve([])
 *     return Promise.reject(new Error(`Unmocked cmd: ${cmd}`))
 *   })
 */

import { vi } from 'vitest'

/** Default invoke mock — every test should override per-command. */
export const mockInvoke = vi.fn(async (cmd: string, _args?: unknown) => {
  throw new Error(`mockInvoke: command ${cmd} not stubbed for this test`)
})

/** Default listen mock — returns a no-op unlisten. */
export const mockListen = vi.fn(async (_event: string, _handler: unknown) => {
  return () => {}
})

/** Reset both mocks. Call this in `beforeEach` if your test stubs them. */
export function resetTauriMocks(): void {
  mockInvoke.mockClear()
  mockListen.mockClear()
}
```

- [ ] **Step 2.3: TS sanity check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: zero errors. (The `@/components/ui/tooltip` import in `render.tsx` should resolve via the existing alias config.)

- [ ] **Step 2.4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/test-utils/render.tsx ui/src/test-utils/mock-tauri.ts
git commit -m "$(cat <<'EOF'
test(utils): add renderWithProviders + mock-tauri helpers

Two utilities that every chat-layer test will use:

- renderWithProviders wraps RTL render() with Jotai Provider +
  Radix Tooltip provider. Returns the rendered result plus a
  scoped userEvent and the store (so tests can assert atom values
  via store.get()).
- mock-tauri provides ready-made vi.fn() mocks for invoke and
  listen. Tests override per-command via mockImplementation.

Re-exports RTL's screen / waitFor / fireEvent / within from
render.tsx so tests have one import line.
EOF
)"
```

---

## Task 3: Test `ChatToolBlock` — rendering matrix

**Files:**
- Create: `ui/src/components/chat/ChatToolBlock.test.tsx`

The component has 4 distinct states: completed-success, completed-error, running, expanded. Lock all four in.

- [ ] **Step 3.1: Read the component to know what to assert**

```bash
cd /Users/ryanliu/Documents/uclaw
grep -nE "Check |AlertTriangle|Loader2|isError|isCompleted" ui/src/components/chat/ChatToolBlock.tsx | head -10
```

Confirm the icons used per state (from PRs #11–#16):
- Completed success → `Check` icon, `text-emerald-500/80` (and `dark:text-emerald-400/80`)
- Completed error → `AlertTriangle`, `text-destructive`, plus row tinted `bg-destructive/[0.04]`
- Running → `Loader2`, animate-spin
- Expanded result → `ToolResultRenderer` mounts under the row

- [ ] **Step 3.2: Create `ui/src/components/chat/ChatToolBlock.test.tsx`**

```tsx
import { describe, it, expect } from 'vitest'
import { ChatToolBlock } from './ChatToolBlock'
import { renderWithProviders, screen } from '@/test-utils/render'

describe('ChatToolBlock', () => {
  const baseProps = {
    toolName: 'bash',
    input: { command: 'ls -a' },
    isCompleted: true,
    animate: false,
    index: 0,
  }

  describe('completed success', () => {
    it('renders a check icon when completed without error', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ok" isError={false} />,
      )
      // The Check icon is the first svg in the row; lucide-react renders it as
      // <svg class="lucide lucide-check ..."> — assert by class fragment.
      const svgs = container.querySelectorAll('svg.lucide')
      const hasCheck = Array.from(svgs).some((s) => s.classList.contains('lucide-check'))
      expect(hasCheck).toBe(true)
    })

    it('does not render AlertTriangle for successful row', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ok" isError={false} />,
      )
      const hasTriangle = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-triangle-alert'))
      expect(hasTriangle).toBe(false)
    })
  })

  describe('completed error', () => {
    it('renders an AlertTriangle icon and tints the row', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="error output" isError={true} />,
      )
      const hasTriangle = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-triangle-alert'))
      expect(hasTriangle).toBe(true)

      // Row container should carry the destructive tint class
      const button = screen.getByRole('button')
      expect(button.className).toMatch(/bg-destructive/)
    })
  })

  describe('running', () => {
    it('renders a Loader2 spinner when not yet completed', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} isCompleted={false} />,
      )
      const hasLoader = Array.from(container.querySelectorAll('svg.lucide'))
        .some((s) => s.classList.contains('lucide-loader-circle'))
      expect(hasLoader).toBe(true)

      // Button should be disabled (no result to expand yet)
      expect(screen.getByRole('button')).toBeDisabled()
    })
  })

  describe('expansion', () => {
    it('expands result panel on click when result present', async () => {
      const { user } = renderWithProviders(
        <ChatToolBlock {...baseProps} result="ls output" isError={false} />,
      )
      // Result not visible initially (expanded panel not rendered)
      expect(screen.queryByText('ls output')).not.toBeInTheDocument()

      await user.click(screen.getByRole('button'))

      // Result panel renders the content (ToolResultRenderer may format it,
      // but the raw text should appear somewhere in the DOM)
      expect(screen.getByText(/ls output/)).toBeInTheDocument()
    })

    it('button is non-clickable when no result', () => {
      const { container } = renderWithProviders(
        <ChatToolBlock {...baseProps} isCompleted={true} result={undefined} />,
      )
      const button = container.querySelector('button')
      expect(button).toBeDisabled()
    })
  })
})
```

- [ ] **Step 3.3: Run the suite**

```bash
cd ui && npx vitest run ChatToolBlock 2>&1 | tail -15
```
Expected: 6 tests passing. If a test fails because the icon class name is slightly different in the installed `lucide-react` version, adjust the assertion (e.g. lucide may use `lucide-loader2` vs `lucide-loader-circle`). Use the actual rendered HTML to confirm:
```bash
cd ui && npx vitest run ChatToolBlock --reporter=verbose 2>&1 | head -40
```

- [ ] **Step 3.4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/chat/ChatToolBlock.test.tsx
git commit -m "test(chat): add ChatToolBlock rendering matrix tests

Six tests covering the four visual states the component supports:
  - completed success → Check icon, no AlertTriangle
  - completed error → AlertTriangle + bg-destructive tint
  - running → Loader2 spinner, button disabled (no expand target)
  - expansion → click reveals result panel; disabled when no result

Locks the visual contract from PRs #11–#16 (the timeline cleanup
and the success/failure differentiation work). Future visual
tweaks must update these tests, preventing accidental regressions
back to the dotless or icon-less variants."
```

---

## Task 4: Test `ChatToolActivityIndicator` — start/result merge

**Files:**
- Create: `ui/src/components/chat/ChatToolActivityIndicator.test.tsx`

The component merges a stream of `start` and `result` events by `toolCallId` into per-tool render. The merge logic is non-trivial — easy to break.

- [ ] **Step 4.1: Create the test file**

```tsx
import { describe, it, expect } from 'vitest'
import { ChatToolActivityIndicator } from './ChatToolActivityIndicator'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ChatToolActivity } from '@/lib/chat-types'

describe('ChatToolActivityIndicator', () => {
  it('renders nothing for an empty array', () => {
    const { container } = renderWithProviders(
      <ChatToolActivityIndicator activities={[]} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders one row per merged toolCallId, not per event', () => {
    // Two tool calls each with start + result → 4 events, 2 visible rows.
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: { command: 'ls' } },
      { toolCallId: 'tc-2', type: 'start', toolName: 'bash', input: { command: 'pwd' } },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: { command: 'ls' }, result: 'a b c', isError: false },
      { toolCallId: 'tc-2', type: 'result', toolName: 'bash', input: { command: 'pwd' }, result: '/home', isError: false },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    // Each ChatToolBlock is a button — count rows.
    expect(screen.getAllByRole('button')).toHaveLength(2)
  })

  it('result event marks the row done (becomes expandable)', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: {}, result: 'output', isError: false },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    expect(screen.getByRole('button')).not.toBeDisabled()
  })

  it('start-only event yields a still-running row (button disabled)', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    expect(screen.getByRole('button')).toBeDisabled()
  })

  it('out-of-order result-before-start still merges by id', () => {
    // Defensive: if for some reason result arrives first (e.g. dropped start),
    // the row should still render and be done.
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: { command: 'foo' }, result: 'ok', isError: false },
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: { command: 'foo' } },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    // Single row, completed (result was set)
    expect(screen.getAllByRole('button')).toHaveLength(1)
    expect(screen.getByRole('button')).not.toBeDisabled()
  })

  it('isError flag from result event propagates to the row tint', () => {
    const activities: ChatToolActivity[] = [
      { toolCallId: 'tc-1', type: 'start', toolName: 'bash', input: {} },
      { toolCallId: 'tc-1', type: 'result', toolName: 'bash', input: {}, result: 'err', isError: true },
    ]
    renderWithProviders(<ChatToolActivityIndicator activities={activities} />)
    const button = screen.getByRole('button')
    expect(button.className).toMatch(/bg-destructive/)
  })
})
```

- [ ] **Step 4.2: Run + commit**

```bash
cd ui && npx vitest run ChatToolActivityIndicator 2>&1 | tail -15
```
Expected: 6 tests passing.

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/chat/ChatToolActivityIndicator.test.tsx
git commit -m "test(chat): add ChatToolActivityIndicator merge tests

Six tests covering the start/result event merge logic that turns a
stream of ChatToolActivity events into per-tool rows:

  - empty input renders nothing
  - merge by toolCallId (4 events → 2 rows)
  - result event makes the row done (expandable)
  - start-only event leaves the row disabled (running)
  - out-of-order result-before-start still merges
  - isError from result event propagates to row tint

Locks the contract that PR #5 (initial port) and PR #9 (live/historical
unification) established. Future refactors of the merge map cannot
silently break event ordering."
```

---

## Task 5: Test `ChatAppearancePopover` — atom interactions

**Files:**
- Create: `ui/src/components/chat/ChatAppearancePopover.test.tsx`

The popover's job is to write atom values + `data-chat-*` attributes on `<html>`. Test both sides.

- [ ] **Step 5.1: Create the test file**

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { ChatAppearancePopover } from './ChatAppearancePopover'
import { renderWithProviders, screen } from '@/test-utils/render'
import { chatFontSizeAtom, chatSerifAtom } from '@/atoms/chat-appearance'

describe('ChatAppearancePopover', () => {
  beforeEach(() => {
    // Clear any data-chat-* state left on <html> by previous tests.
    document.documentElement.removeAttribute('data-chat-font-size')
    document.documentElement.removeAttribute('data-chat-serif')
  })

  it('renders a trigger button by default (popover closed)', () => {
    renderWithProviders(<ChatAppearancePopover />)
    // Popover trigger is a button; the content panel is not yet in the DOM.
    expect(screen.getByRole('button')).toBeInTheDocument()
    // Font-size buttons inside the panel are not visible until trigger is clicked.
    expect(screen.queryByText('小')).not.toBeInTheDocument()
  })

  it('opens the popover on trigger click and shows three font-size choices', async () => {
    const { user } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))
    // Three size buttons inside the panel
    expect(screen.getByText('小')).toBeInTheDocument()
    expect(screen.getByText('中')).toBeInTheDocument()
    expect(screen.getByText('大')).toBeInTheDocument()
  })

  it('clicking a size button updates the atom and the html data attribute', async () => {
    const { user, store } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))   // open popover
    await user.click(screen.getByText('大'))
    expect(store.get(chatFontSizeAtom)).toBe('lg')
    expect(document.documentElement.getAttribute('data-chat-font-size')).toBe('lg')
  })

  it('toggling serif switch updates the atom and the html data attribute', async () => {
    const { user, store } = renderWithProviders(<ChatAppearancePopover />)
    await user.click(screen.getByRole('button'))   // open popover
    // The serif Switch is a Radix component — find it by accessible name.
    const switchEl = screen.getByRole('switch')
    expect(store.get(chatSerifAtom)).toBe(false)
    await user.click(switchEl)
    expect(store.get(chatSerifAtom)).toBe(true)
    expect(document.documentElement.getAttribute('data-chat-serif')).toBe('true')
  })
})
```

- [ ] **Step 5.2: Run + commit**

```bash
cd ui && npx vitest run ChatAppearancePopover 2>&1 | tail -15
```
Expected: 4 tests passing.

If `screen.getByText('小')` fails because Radix renders the popover content into a portal that RTL still sees by default — verify by adding `screen.debug()` in the test temporarily. Most setups work; if there's an issue, wrap with `await screen.findByText('小')` to wait for the portal mount.

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/chat/ChatAppearancePopover.test.tsx
git commit -m "test(chat): add ChatAppearancePopover atom + DOM tests

Four tests covering:
  - trigger button is rendered, popover content hidden initially
  - clicking trigger reveals three font-size choices
  - selecting a size updates chatFontSizeAtom + html[data-chat-font-size]
  - toggling serif updates chatSerifAtom + html[data-chat-serif]

Both sides of the popover's contract — Jotai atom and DOM attribute —
are now locked. Future changes to either path (e.g. moving the atom
or renaming the data attribute) will fail this test loudly."
```

---

## Task 6: Test `ScrollPositionManager` — id-change → scroll-to-bottom

**Files:**
- Create: `ui/src/hooks/useScrollPositionMemory.test.tsx`

The hook is the entry point for "switch session → land at bottom of history". Small but high-leverage.

- [ ] **Step 6.1: Create the test file**

```tsx
import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { ScrollPositionManager } from './useScrollPositionMemory'
import { renderWithProviders } from '@/test-utils/render'

// We need a controlled ConversationContext.Provider to verify the hook
// calls scrollToBottom. Mock the module so we can intercept the context.

vi.mock('@/components/ai-elements/conversation', async (importOriginal) => {
  const actual: any = await importOriginal()
  // Replace the hook with a controllable spy
  return {
    ...actual,
    useConversationContext: vi.fn(),
  }
})

import { useConversationContext } from '@/components/ai-elements/conversation'

describe('ScrollPositionManager', () => {
  it('does nothing when context is null', () => {
    ;(useConversationContext as any).mockReturnValue(null)
    // Should render without error and produce nothing
    const { container } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={true} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('does nothing when ready=false', () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    renderWithProviders(<ScrollPositionManager id="session-1" ready={false} />)
    // The hook waits for ready before scrolling.
    expect(scrollToBottom).not.toHaveBeenCalled()
  })

  it('calls scrollToBottom when id is set and ready flips true', async () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    const { rerender } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={false} />,
    )
    // Wait one frame — useEffect with requestAnimationFrame
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).not.toHaveBeenCalled()

    // Flip ready to true → scroll fires on next frame
    rerender(<ScrollPositionManager id="session-1" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledWith('auto')
  })

  it('calls scrollToBottom again when id changes', async () => {
    const scrollToBottom = vi.fn()
    ;(useConversationContext as any).mockReturnValue({
      scrollRef: React.createRef<HTMLDivElement>(),
      viewportEl: null,
      scrollToBottom,
    })
    const { rerender } = renderWithProviders(
      <ScrollPositionManager id="session-1" ready={true} />,
    )
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(1)

    // Same id → no extra call
    rerender(<ScrollPositionManager id="session-1" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(1)

    // New id → another call
    rerender(<ScrollPositionManager id="session-2" ready={true} />)
    await new Promise((r) => requestAnimationFrame(r))
    expect(scrollToBottom).toHaveBeenCalledTimes(2)
  })
})
```

- [ ] **Step 6.2: Run + commit**

```bash
cd ui && npx vitest run useScrollPositionMemory 2>&1 | tail -15
```
Expected: 4 tests passing.

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/hooks/useScrollPositionMemory.test.tsx
git commit -m "test(hooks): add ScrollPositionManager tests

Four tests covering the hook that drives 'switch session → see latest
message immediately' (PR #4):

  - null context: renders without crashing
  - not ready: does not call scrollToBottom
  - ready flips true: scrollToBottom fires once with 'auto'
  - id changes: another scrollToBottom; same id does not re-fire

Locks the contract that other UI plans (P11 onboarding, P7 edit/regen)
will rely on for predictable initial-scroll behavior."
```

---

## Task 7: Final verification + push + PR

- [ ] **Step 7.1: Run the full suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test 2>&1 | tail -10
```
Expected output ends with:
```
Test Files  4 passed (4)
     Tests  20 passed (20)
```

(6 + 6 + 4 + 4 = 20.)

- [ ] **Step 7.2: TS + Vite still green**

```bash
npx tsc --noEmit 2>&1 | head -3 && npx vite build 2>&1 | tail -3
```

- [ ] **Step 7.3: Coverage report sanity check**

```bash
npx vitest run --coverage 2>&1 | tail -15
```
Expected: coverage report prints. Coverage % isn't important to gate yet — we have 4 files tested out of dozens. The point is the report works.

- [ ] **Step 7.4: Push branch + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw
git push -u origin claude/p3-frontend-tests
gh pr create --title "P3: Frontend test infrastructure (Vitest + RTL + 20 starter tests)" --body "$(cat <<'EOF'
## Summary

Implements roadmap P3. Stands up the frontend test stack and ships a starter suite covering the chat layer that PRs #21 + #24 just touched.

## Stack

- vitest@^1.6 + @vitest/ui — runner, parallel by default
- @testing-library/react@^16, @testing-library/jest-dom, @testing-library/user-event
- jsdom@^25 — DOM env

All `devDependencies` — no runtime impact.

## Configuration

- `ui/vitest.config.ts` extends `vite.config.ts` via `mergeConfig` so `@/` alias resolves identically
- `ui/src/test-utils/setup.ts` — jest-dom matchers, RTL cleanup, jsdom polyfills (`matchMedia`, `ResizeObserver`)
- `ui/src/test-utils/render.tsx` — `renderWithProviders` (Jotai store + Radix Tooltip provider) plus re-exports of RTL screen/waitFor/etc.
- `ui/src/test-utils/mock-tauri.ts` — vi.fn mocks for `invoke` / `listen`

## Starter test files (20 tests, ~1.2s suite)

| File | Tests | Locks |
|---|---|---|
| `ChatToolBlock.test.tsx` | 6 | Visual states from PRs #11–#16 (Check/AlertTriangle/Loader2 icons; expansion behavior) |
| `ChatToolActivityIndicator.test.tsx` | 6 | start/result merge logic from PRs #5 + #9 |
| `ChatAppearancePopover.test.tsx` | 4 | Atom + DOM-attribute writes |
| `useScrollPositionMemory.test.tsx` | 4 | id-change → scrollToBottom contract from PR #4 |

## Scripts added

- `npm test` — single CI-style run
- `npm run test:watch` — dev, re-runs on save
- `npm run test:ui` — browser-based dashboard
- `npm run test:coverage` — v8 coverage report

## What this unblocks

Every later UI plan (P4 search, P5 cost dashboard, P6 permission UI, P11 onboarding) can now add tests in the same harness instead of inventing their own.

## Test plan

- [x] `npm test` — 20/20 pass
- [x] `tsc --noEmit` clean
- [x] `vite build` succeeds
- [x] Coverage report generates without errors

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria (cumulative)

- ✅ `npm test` runs all 4 suites (20 tests) and passes
- ✅ `npm run test:watch` activates watch mode without errors
- ✅ `tsc --noEmit` clean
- ✅ `vite build` succeeds
- ✅ Coverage report generates via `npm run test:coverage`
- ✅ Each task ships its own commit so the PR is bisectable

## Out of scope

- Tests for the agent view (`AgentMessages`, `AgentMessageItem`, etc.) — that's a follow-up; the chat view's tests already cover the shared `ChatToolBlock` / `ChatToolActivityIndicator` since both paths use them.
- Backend Rust integration tests — that was intentionally deferred in the LLM timeout fix's plan; the classifier has unit tests and that's enough for now.
- E2E tests with Playwright — separate plan if/when needed.
- Per-PR coverage thresholds — building toward that requires more tests than 4 files; keep coverage advisory until P4–P11 add coverage organically.

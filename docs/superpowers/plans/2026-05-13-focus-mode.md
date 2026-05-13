# Focus Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Branch:** `claude/focus-mode` (HEAD on `f62035e` = spec commit). At the start of EVERY task run `git branch --show-current` to confirm — if it's not `claude/focus-mode`, immediately report BLOCKED.

**Goal:** Hide LeftSidebar and RightSidePanel when a preview is open and the user activates Focus Mode (Alt+F or button); reveal either panel as a floating island when the cursor approaches that screen edge, with a polished three-layer glow indicator.

**Architecture:** Pure-UI feature with three jotai atoms as state core, three hooks for behavior (shortcut, hot zone, auto-exit), four React components for the visual layer (overlay, two-side island wrapper, edge glow, header button), and a one-line `useFocusModeShortcut` mount + conditional render in AppShell. Spec at [`docs/superpowers/specs/2026-05-13-focus-mode-design.md`](../specs/2026-05-13-focus-mode-design.md).

**Tech Stack:** TypeScript · React 18 · Jotai · Framer Motion (`motion/react`) · Tailwind · Vitest + RTL + jsdom.

---

## Constraints — every implementer must respect these

1. **Zero backend changes.** Pure `ui/` work. If implementation seems to require a Rust change → **BLOCKED**, escalate.
2. **Don't touch LeftSidebar / RightSidePanel internals.** They are passed as `children` to the `FloatingIsland` wrapper unchanged.
3. **Theme tokens only** — `hsl(var(--focus-glow))` / `hsl(var(--focus-glow-bright))` / `bg-popover` / `text-muted-foreground`. No hardcoded hex.
4. **Each commit must be green** — `npx tsc --noEmit` clean AND `npm test -- --run` all-pass.
5. **One commit per task** — bisectable.
6. **Branch hygiene** — confirm `claude/focus-mode` at start, before commit, after commit.

---

## File Structure

**New files (12 — 8 implementation + 4 tests)**

| File | Responsibility |
|---|---|
| `ui/src/atoms/focus-mode-atoms.ts` | 4 atoms (focusMode / revealSide / pinned / mousePos) + 2 actions (toggle, exit) |
| `ui/src/atoms/focus-mode-atoms.test.ts` | Atom behavior tests |
| `ui/src/hooks/useFocusModeShortcut.ts` | Alt+F binding via `useShortcut` |
| `ui/src/hooks/useFocusModeAutoExit.ts` | Watch `previewPanelOpenAtom` → auto-exit |
| `ui/src/hooks/useFocusModeAutoExit.test.ts` | Auto-exit branch tests |
| `ui/src/lib/focus-mode-geometry.ts` | Pure helper: `isInsideIslandRect()` bounding-box check |
| `ui/src/lib/focus-mode-geometry.test.ts` | Helper tests |
| `ui/src/hooks/useFocusModeHotzone.ts` | mousemove listener + 200ms timer + reveal state machine |
| `ui/src/hooks/useFocusModeHotzone.test.ts` | Hot zone state machine tests |
| `ui/src/components/focus-mode/FloatingIsland.tsx` | Single-side island wrapper (Framer Motion + click-outside pin) |
| `ui/src/components/focus-mode/GlowIndicator.tsx` | Three-layer breathing glow + Y trace |
| `ui/src/components/focus-mode/FocusModeOverlay.tsx` | Mounts hooks + composes 2 islands + 2 glow indicators |
| `ui/src/components/focus-mode/FocusModeButton.tsx` | Maximize2/Minimize2 toggle for PreviewHeader |
| `ui/src/components/focus-mode/FocusModeButton.test.tsx` | Button rendering + click tests |

**Modified files (4)**

| File | Change |
|---|---|
| `ui/src/lib/shortcut-defaults.ts` | Register `'toggle-focus-mode'` definition |
| `ui/src/styles/globals.css` | 12 theme blocks × 2 tokens + 4 `.focus-glow-*` classes + 3 keyframes |
| `ui/src/components/preview/PreviewHeader.tsx` | Mount `<FocusModeButton />` left of action trio |
| `ui/src/components/app-shell/AppShell.tsx` | Mount `useFocusModeShortcut()` + `<FocusModeOverlay />`; conditional render of LeftSidebar / RightSidePanel |

---

## Task Map

| # | Task | Model | Tests added | Files touched |
|---|---|---|---|---|
| 1 | Atoms + tests | haiku | 4 | 2 new |
| 2 | Shortcut registration | haiku | 0 | 1 modified |
| 3 | `useFocusModeShortcut` + `useFocusModeAutoExit` + tests | sonnet | 3 | 2 new + 1 test |
| 4 | `isInsideIslandRect` helper + tests | sonnet | 5 | 2 new |
| 5 | `useFocusModeHotzone` + tests | sonnet | 6 | 2 new |
| 6 | globals.css theme tokens + glow CSS | haiku | 0 | 1 modified |
| 7 | `FloatingIsland` component | sonnet | 2 | 1 new (+ test slipped in) |
| 8 | `GlowIndicator` component | sonnet | 1 | 1 new (+ test) |
| 9 | `FocusModeOverlay` component | sonnet | 1 | 1 new (+ test) |
| 10 | `FocusModeButton` + tests | sonnet | 2 | 2 new |
| 11 | PreviewHeader integration | sonnet | 0 | 1 modified |
| 12 | AppShell integration | sonnet | 0 | 1 modified |
| 13 | Final verification | sonnet | 0 | none |

**Test count budget:** ~26 new tests (spec budgeted 15, plan adds extra component-level smoke tests for stability). Current UI baseline: 363 → target ~389.

---

## Task 1: Create focus-mode atoms

**Model:** haiku (mechanical — atoms + simple action tests)

**Files:**
- Create: `ui/src/atoms/focus-mode-atoms.ts`
- Test: `ui/src/atoms/focus-mode-atoms.test.ts`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

Expected: `claude/focus-mode`. If not, **BLOCKED**.

- [ ] **Step 1: Write the failing test file**

Create `ui/src/atoms/focus-mode-atoms.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
  toggleFocusModeAction,
  exitFocusModeAction,
} from './focus-mode-atoms'

describe('focus-mode-atoms', () => {
  it('defaults to off / null / unpinned / origin', () => {
    const store = createStore()
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
    expect(store.get(focusMousePosAtom)).toEqual({ x: 0, y: 0 })
  })

  it('toggleFocusModeAction flips focusModeAtom', () => {
    const store = createStore()
    store.set(toggleFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(true)
    store.set(toggleFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(false)
  })

  it('toggling OFF clears reveal + pin state', () => {
    const store = createStore()
    store.set(toggleFocusModeAction)             // → on
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    store.set(toggleFocusModeAction)             // → off, must clean up
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })

  it('exitFocusModeAction forces every flag back to defaults', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'right')
    store.set(focusRevealPinnedAtom, true)
    store.set(exitFocusModeAction)
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/atoms/focus-mode-atoms.test.ts 2>&1 | tail -10
```

Expected: FAIL with `Cannot find module './focus-mode-atoms'`.

- [ ] **Step 3: Create the implementation**

Create `ui/src/atoms/focus-mode-atoms.ts`:

```ts
/**
 * focus-mode-atoms — state for Focus Mode (hides LeftSidebar +
 * RightSidePanel when a preview is open; reveals them on edge hover).
 *
 *   focusModeAtom           : boolean    — global on/off
 *   focusRevealSideAtom     : 'left' | 'right' | null — which island is shown
 *   focusRevealPinnedAtom   : boolean    — click-inside latch
 *   focusMousePosAtom       : { x, y }   — last mousemove (drives glow opacity + Y trace)
 *
 *   toggleFocusModeAction   — flip; auto-cleans reveal/pin when going OFF
 *   exitFocusModeAction     — force everything to defaults (used by autoExit)
 */

import { atom } from 'jotai'

export const focusModeAtom = atom<boolean>(false)
export const focusRevealSideAtom = atom<'left' | 'right' | null>(null)
export const focusRevealPinnedAtom = atom<boolean>(false)
export const focusMousePosAtom = atom<{ x: number; y: number }>({ x: 0, y: 0 })

export const toggleFocusModeAction = atom(null, (get, set) => {
  const next = !get(focusModeAtom)
  set(focusModeAtom, next)
  if (!next) {
    // Going OFF: scrub transient reveal/pin state so the next ON starts clean.
    set(focusRevealSideAtom, null)
    set(focusRevealPinnedAtom, false)
  }
})

export const exitFocusModeAction = atom(null, (_get, set) => {
  set(focusModeAtom, false)
  set(focusRevealSideAtom, null)
  set(focusRevealPinnedAtom, false)
})
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/atoms/focus-mode-atoms.test.ts 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 5: Run tsc + full test suite for safety**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: tsc clean. Total tests pass (363 → 367).

- [ ] **Step 6: Commit (confirm branch first)**

```bash
git branch --show-current   # must be claude/focus-mode
git add ui/src/atoms/focus-mode-atoms.ts ui/src/atoms/focus-mode-atoms.test.ts
git commit -m "feat(focus-mode): atoms + actions (toggle / exit)

3 jotai atoms power Focus Mode: focusModeAtom (global on/off),
focusRevealSideAtom (which floating island shows), focusRevealPinnedAtom
(click-inside latch). focusMousePosAtom mirrors the last mousemove so
the glow + Y trace can read it without re-mounting listeners.

toggleFocusModeAction scrubs reveal/pin when going OFF so the next ON
starts from a clean state. exitFocusModeAction is the harder
exit-everything used by the watcher hook (added in next task).
"
git branch --show-current   # confirm still on claude/focus-mode
```

---

## Task 2: Register `toggle-focus-mode` shortcut

**Model:** haiku (one-line config addition)

**Files:**
- Modify: `ui/src/lib/shortcut-defaults.ts` (append entry to `SHORTCUT_DEFINITIONS`)

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current   # claude/focus-mode
```

- [ ] **Step 1: Insert new shortcut entry**

Open `ui/src/lib/shortcut-defaults.ts`. After the existing `'toggle-side-panel'` entry (currently ends at line 140 with `},`), add a new entry inside the `SHORTCUT_DEFINITIONS` array. The array's closing `]` is on line 141. The new entry goes between line 140's `},` and line 141's `]`:

```ts
  {
    id: 'toggle-focus-mode',
    label: '专注模式',
    group: 'Agent',
    mac: 'Alt+F',
    win: 'Alt+F',
  },
```

After insertion, the relevant section reads:

```ts
  {
    id: 'toggle-side-panel',
    label: '切换侧面板',
    group: 'Agent',
    mac: 'Cmd+Shift+B',
    win: 'Ctrl+Shift+B',
  },
  {
    id: 'toggle-focus-mode',
    label: '专注模式',
    group: 'Agent',
    mac: 'Alt+F',
    win: 'Alt+F',
  },
]
```

- [ ] **Step 2: Verify tsc + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: tsc clean, all tests still pass.

- [ ] **Step 3: Commit**

```bash
git branch --show-current
git add ui/src/lib/shortcut-defaults.ts
git commit -m "feat(focus-mode): register toggle-focus-mode shortcut (Alt+F)

Alt+F on both Mac and Windows. On Mac, useShortcut's default
preventDefault: true (useShortcut.ts:80) blocks the OS-level
Option+F → ƒ character insertion, so this binding is safe to fire
even when an editor is focused — which is exactly the desired
behaviour (user can enter focus mode without clicking out of the
editor first).
"
```

---

## Task 3: `useFocusModeShortcut` + `useFocusModeAutoExit` hooks

**Model:** sonnet (atom-driven hooks + watcher logic + side-effect test patterns)

**Files:**
- Create: `ui/src/hooks/useFocusModeShortcut.ts`
- Create: `ui/src/hooks/useFocusModeAutoExit.ts`
- Test: `ui/src/hooks/useFocusModeAutoExit.test.ts`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write the failing test**

Create `ui/src/hooks/useFocusModeAutoExit.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useFocusModeAutoExit } from './useFocusModeAutoExit'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useFocusModeAutoExit', () => {
  it('exits Focus Mode when preview closes', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(previewPanelOpenAtom, true)
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(true)
    act(() => store.set(previewPanelOpenAtom, false))
    expect(store.get(focusModeAtom)).toBe(false)
  })

  it('does not exit while preview stays open', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(previewPanelOpenAtom, true)
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(true)
  })

  it('corrects orphan focus state on mount (focus=true but no preview)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    store.set(previewPanelOpenAtom, false)   // orphan: focus on but no preview
    renderHook(() => useFocusModeAutoExit(), { wrapper: wrapper(store) })
    expect(store.get(focusModeAtom)).toBe(false)
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusRevealPinnedAtom)).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/useFocusModeAutoExit.test.ts 2>&1 | tail -10
```

Expected: FAIL with `Cannot find module './useFocusModeAutoExit'`.

- [ ] **Step 3: Create useFocusModeShortcut**

Create `ui/src/hooks/useFocusModeShortcut.ts`:

```ts
/**
 * useFocusModeShortcut — Alt+F → toggle Focus Mode.
 *
 * Mount once at AppShell top. preventDefault is true by default
 * (useShortcut.ts:80) so Mac's Option+F → ƒ character insertion is
 * blocked automatically — the binding works even when focus is
 * inside a code editor or chat input.
 */

import { useSetAtom } from 'jotai'
import { useShortcut } from './useShortcut'
import { toggleFocusModeAction } from '@/atoms/focus-mode-atoms'

export function useFocusModeShortcut(): void {
  const toggle = useSetAtom(toggleFocusModeAction)
  useShortcut({
    id: 'toggle-focus-mode',
    handler: () => toggle(),
  })
}
```

- [ ] **Step 4: Create useFocusModeAutoExit**

Create `ui/src/hooks/useFocusModeAutoExit.ts`:

```ts
/**
 * useFocusModeAutoExit — when the preview panel closes, Focus Mode
 * loses its reason to exist; force-exit so the user isn't stranded
 * in a sidebars-hidden state with no preview to focus on.
 *
 * Also runs the same check on mount to scrub any orphan state left
 * over from a previous session (e.g. preview was closed in another
 * workspace while Focus Mode was still on globally).
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  focusModeAtom,
  exitFocusModeAction,
} from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

export function useFocusModeAutoExit(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const exit = useSetAtom(exitFocusModeAction)

  React.useEffect(() => {
    if (focusMode && !previewOpen) exit()
  }, [focusMode, previewOpen, exit])
}
```

- [ ] **Step 5: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/useFocusModeAutoExit.test.ts 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: 3 new tests pass. tsc clean. Suite green (367 → 370).

- [ ] **Step 6: Commit**

```bash
git branch --show-current
git add ui/src/hooks/useFocusModeShortcut.ts ui/src/hooks/useFocusModeAutoExit.ts ui/src/hooks/useFocusModeAutoExit.test.ts
git commit -m "feat(focus-mode): Alt+F shortcut + preview-close auto-exit hooks

useFocusModeShortcut binds Alt+F via the existing useShortcut helper.
preventDefault: true (default) handles Mac's Option+F → ƒ insertion.

useFocusModeAutoExit watches previewPanelOpenAtom. When focus mode is
on but no preview is open (either because user closed the preview, or
mount-time orphan state), it fires exitFocusModeAction to reset
everything to defaults.
"
```

---

## Task 4: `isInsideIslandRect` geometry helper

**Model:** sonnet (pure function but with edge-case judgement)

**Files:**
- Create: `ui/src/lib/focus-mode-geometry.ts`
- Test: `ui/src/lib/focus-mode-geometry.test.ts`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write the failing test**

Create `ui/src/lib/focus-mode-geometry.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import {
  isInsideIslandRect,
  ISLAND_LEFT_WIDTH,
  ISLAND_RIGHT_WIDTH,
  ISLAND_EDGE_GAP,
  TOP_EXCLUDE,
} from './focus-mode-geometry'

const W = 1440
const H = 900

describe('isInsideIslandRect', () => {
  it('returns true for a point inside the LEFT island box', () => {
    // Left island: x in [12, 12+280=292], y in [12, 900-12=888]
    expect(isInsideIslandRect('left', 150, 400, W, H)).toBe(true)
  })

  it('returns false for a point outside the LEFT island (to the right of it)', () => {
    expect(isInsideIslandRect('left', 400, 400, W, H)).toBe(false)
  })

  it('returns true for a point inside the RIGHT island box', () => {
    // Right island: x in [W-12-400=1028, W-12=1428]
    expect(isInsideIslandRect('right', 1200, 400, W, H)).toBe(true)
  })

  it('returns false for the WRONG side (mouse on right but checking left)', () => {
    expect(isInsideIslandRect('left', 1200, 400, W, H)).toBe(false)
  })

  it('excludes the top TOP_EXCLUDE band', () => {
    // y < 84 should never count
    expect(isInsideIslandRect('left', 150, 50, W, H)).toBe(false)
    expect(isInsideIslandRect('right', 1200, 50, W, H)).toBe(false)
  })

  it('exposes geometry constants used by the overlay layout', () => {
    expect(ISLAND_LEFT_WIDTH).toBe(280)
    expect(ISLAND_RIGHT_WIDTH).toBe(400)
    expect(ISLAND_EDGE_GAP).toBe(12)
    expect(TOP_EXCLUDE).toBe(84)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/lib/focus-mode-geometry.test.ts 2>&1 | tail -10
```

Expected: FAIL with `Cannot find module './focus-mode-geometry'`.

- [ ] **Step 3: Create the helper**

Create `ui/src/lib/focus-mode-geometry.ts`:

```ts
/**
 * focus-mode-geometry — pure helpers for the Focus Mode overlay
 * geometry. Kept here (not inside the hotzone hook) so they can be
 * unit-tested in isolation and reused by FloatingIsland for layout.
 */

/** Width of the LEFT floating island in CSS px (mirrors LeftSidebar default). */
export const ISLAND_LEFT_WIDTH = 280
/** Width of the RIGHT floating island in CSS px (mirrors RightSidePanel fixed width). */
export const ISLAND_RIGHT_WIDTH = 400
/** Gap between island edge and screen edge. */
export const ISLAND_EDGE_GAP = 12
/** Top band reserved for the titlebar drag region + TabBar — hot zone is excluded here. */
export const TOP_EXCLUDE = 84
/** Hot zone width near the screen edge (in CSS px). */
export const HOT_ZONE_WIDTH = 32

/**
 * Returns true if `(x, y)` falls inside the bounding box of the floating
 * island on `side`. Used by the hotzone hook to decide whether the mouse
 * is still "in the island region" (which suppresses the leave timer).
 *
 * Top band (y < TOP_EXCLUDE) is always rejected so the hot zone never
 * fights with the macOS traffic-light buttons / window drag.
 */
export function isInsideIslandRect(
  side: 'left' | 'right',
  x: number,
  y: number,
  windowWidth: number,
  windowHeight: number,
): boolean {
  if (y < TOP_EXCLUDE) return false
  if (y > windowHeight - ISLAND_EDGE_GAP) return false
  if (side === 'left') {
    return x >= ISLAND_EDGE_GAP && x <= ISLAND_EDGE_GAP + ISLAND_LEFT_WIDTH
  }
  const rightStart = windowWidth - ISLAND_EDGE_GAP - ISLAND_RIGHT_WIDTH
  const rightEnd = windowWidth - ISLAND_EDGE_GAP
  return x >= rightStart && x <= rightEnd
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/lib/focus-mode-geometry.test.ts 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: 6 tests pass. tsc clean. (Note: 6 test cases, 5 was a target; one of them tests the exported constants which is more like a snapshot — both count.)

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/lib/focus-mode-geometry.ts ui/src/lib/focus-mode-geometry.test.ts
git commit -m "feat(focus-mode): isInsideIslandRect geometry helper + constants

Pure function returning whether a point is inside the floating-island
bounding box on either side. Constants (widths / gap / top exclude)
also exported so FloatingIsland's layout reads the SAME values that
the hot zone uses for hit-testing — no drift.
"
```

---

## Task 5: `useFocusModeHotzone` hook

**Model:** sonnet (state machine + timer + window event listener + 6 test cases)

**Files:**
- Create: `ui/src/hooks/useFocusModeHotzone.ts`
- Test: `ui/src/hooks/useFocusModeHotzone.test.ts`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write the failing test**

Create `ui/src/hooks/useFocusModeHotzone.test.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import * as React from 'react'
import { useFocusModeHotzone } from './useFocusModeHotzone'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

function fireMouseMove(clientX: number, clientY: number): void {
  window.dispatchEvent(new MouseEvent('mousemove', { clientX, clientY }))
}

const ORIG_W = window.innerWidth
const ORIG_H = window.innerHeight

function setViewport(w: number, h: number): void {
  Object.defineProperty(window, 'innerWidth', { value: w, writable: true, configurable: true })
  Object.defineProperty(window, 'innerHeight', { value: h, writable: true, configurable: true })
}

describe('useFocusModeHotzone', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setViewport(1440, 900)
  })
  afterEach(() => {
    vi.useRealTimers()
    setViewport(ORIG_W, ORIG_H)
  })

  it('reveals left when mouse enters left hot zone (x <= 32, y > 84)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 200))
    expect(store.get(focusRevealSideAtom)).toBe('left')
  })

  it('reveals right when mouse enters right hot zone', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(1430, 200))   // 1440 - 10 = right hot zone
    expect(store.get(focusRevealSideAtom)).toBe('right')
  })

  it('does NOT reveal when y < TOP_EXCLUDE (84)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 50))      // in left hot zone X-wise, but y < 84
    expect(store.get(focusRevealSideAtom)).toBeNull()
  })

  it('starts 200ms leave timer when mouse leaves the island/hot-zone region', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 200))         // reveal left
    expect(store.get(focusRevealSideAtom)).toBe('left')
    act(() => fireMouseMove(600, 200))        // mouse leaves region
    expect(store.get(focusRevealSideAtom)).toBe('left')  // not yet
    act(() => vi.advanceTimersByTime(200))
    expect(store.get(focusRevealSideAtom)).toBeNull()
  })

  it('cancels the leave timer if mouse returns to the region in time', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 200))
    act(() => fireMouseMove(600, 200))        // leave region; 200ms timer starts
    act(() => vi.advanceTimersByTime(100))
    act(() => fireMouseMove(150, 200))        // back inside island
    act(() => vi.advanceTimersByTime(200))    // full window passes
    expect(store.get(focusRevealSideAtom)).toBe('left')  // still revealed
  })

  it('pinned state prevents the leave timer from hiding the island', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    store.set(focusRevealSideAtom, 'left')
    store.set(focusRevealPinnedAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(600, 200))        // far from any reveal region
    act(() => vi.advanceTimersByTime(500))
    expect(store.get(focusRevealSideAtom)).toBe('left')  // pinned holds
  })

  it('updates focusMousePosAtom on every mousemove (drives glow)', () => {
    const store = createStore()
    store.set(focusModeAtom, true)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(500, 300))
    expect(store.get(focusMousePosAtom)).toEqual({ x: 500, y: 300 })
  })

  it('does nothing when Focus Mode is OFF', () => {
    const store = createStore()
    store.set(focusModeAtom, false)
    renderHook(() => useFocusModeHotzone(), { wrapper: wrapper(store) })
    act(() => fireMouseMove(20, 200))
    expect(store.get(focusRevealSideAtom)).toBeNull()
    expect(store.get(focusMousePosAtom)).toEqual({ x: 0, y: 0 })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/useFocusModeHotzone.test.ts 2>&1 | tail -10
```

Expected: FAIL with `Cannot find module './useFocusModeHotzone'`.

- [ ] **Step 3: Implement the hook**

Create `ui/src/hooks/useFocusModeHotzone.ts`:

```ts
/**
 * useFocusModeHotzone — drives the focusRevealSideAtom state machine
 * from a single global mousemove listener.
 *
 * Rules:
 *   - When the mouse enters the left/right hot zone (≤ HOT_ZONE_WIDTH px
 *     from edge, y > TOP_EXCLUDE) → reveal that side immediately.
 *   - When the mouse is inside the OPEN island's bounding box (or its
 *     hot zone, treated as a union region) → keep revealed.
 *   - When the mouse leaves that union region → start a 200ms timer →
 *     reveal = null.
 *   - Mouse re-entering the region before the timer fires cancels it.
 *   - When pinned === true → the leave timer is suppressed; pinned is
 *     cleared by FloatingIsland's click-outside handler.
 *   - When Focus Mode is OFF → the listener is not registered at all.
 *
 * Mouse position is mirrored into focusMousePosAtom every move so the
 * glow indicator can read it without subscribing to mousemove twice.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'
import {
  isInsideIslandRect,
  HOT_ZONE_WIDTH,
  TOP_EXCLUDE,
} from '@/lib/focus-mode-geometry'

const LEAVE_DELAY_MS = 200

export function useFocusModeHotzone(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const setReveal = useSetAtom(focusRevealSideAtom)
  const setMouse = useSetAtom(focusMousePosAtom)
  // Reads via refs so the listener closure stays stable across renders.
  const revealRef = React.useRef<'left' | 'right' | null>(null)
  const pinnedRef = React.useRef(false)
  const reveal = useAtomValue(focusRevealSideAtom)
  const pinned = useAtomValue(focusRevealPinnedAtom)
  React.useEffect(() => { revealRef.current = reveal }, [reveal])
  React.useEffect(() => { pinnedRef.current = pinned }, [pinned])

  React.useEffect(() => {
    if (!focusMode) return

    let leaveTimer: ReturnType<typeof setTimeout> | null = null
    const clearLeaveTimer = () => {
      if (leaveTimer !== null) {
        clearTimeout(leaveTimer)
        leaveTimer = null
      }
    }

    const onMove = (e: MouseEvent) => {
      // Mirror mouse position for the glow indicator.
      setMouse({ x: e.clientX, y: e.clientY })

      if (pinnedRef.current) return  // pinned freezes the reveal state

      const w = window.innerWidth
      const h = window.innerHeight
      const inLeftZone =
        e.clientX <= HOT_ZONE_WIDTH && e.clientY >= TOP_EXCLUDE
      const inRightZone =
        e.clientX >= w - HOT_ZONE_WIDTH && e.clientY >= TOP_EXCLUDE
      const inLeftIsland = isInsideIslandRect('left', e.clientX, e.clientY, w, h)
      const inRightIsland = isInsideIslandRect('right', e.clientX, e.clientY, w, h)

      const wantLeft = inLeftZone || inLeftIsland
      const wantRight = inRightZone || inRightIsland

      if (wantLeft) {
        clearLeaveTimer()
        if (revealRef.current !== 'left') setReveal('left')
      } else if (wantRight) {
        clearLeaveTimer()
        if (revealRef.current !== 'right') setReveal('right')
      } else if (revealRef.current !== null) {
        // Mouse left the union region — schedule hide.
        if (leaveTimer === null) {
          leaveTimer = setTimeout(() => {
            leaveTimer = null
            setReveal(null)
          }, LEAVE_DELAY_MS)
        }
      }
    }

    window.addEventListener('mousemove', onMove)
    return () => {
      window.removeEventListener('mousemove', onMove)
      clearLeaveTimer()
    }
  }, [focusMode, setReveal, setMouse])
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/useFocusModeHotzone.test.ts 2>&1 | tail -15
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: 8 tests pass (slightly more than the 6 budgeted; the extra two — focusMousePosAtom update and focus-off no-op — are cheap regression guards). tsc clean.

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/hooks/useFocusModeHotzone.ts ui/src/hooks/useFocusModeHotzone.test.ts
git commit -m "feat(focus-mode): hotzone hook with 200ms leave timer + pin guard

Single global mousemove listener; only registers while focusModeAtom
is true. Drives focusRevealSideAtom into left / right / null based on
the union of (hot zone, island bounding box).

When the mouse leaves the union, a 200ms timer schedules the hide;
re-entry cancels it. Pinned state suppresses the timer entirely so a
user filling out a form inside the island never loses it.

focusMousePosAtom is mirrored every move so GlowIndicator can read it
without registering a second mousemove listener.
"
```

---

## Task 6: globals.css — theme tokens + glow CSS

**Model:** haiku (CSS-only, mechanical: 12 themes × 2 tokens + 4 classes + 3 keyframes)

**Files:**
- Modify: `ui/src/styles/globals.css`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Add `--focus-glow` / `--focus-glow-bright` to each theme block**

For each of the 12 theme blocks in `ui/src/styles/globals.css`, add the two tokens (paste before the closing `}` of each block). Below shows the exact pair to insert for each theme — find each block by the `.theme-NAME {` or `:root {` / `.dark {` selector listed in spec § Theme色 token. Insertion text:

```css
/* :root */
  --focus-glow: 200 80% 55%;
  --focus-glow-bright: 200 90% 65%;

/* .dark */
  --focus-glow: 200 85% 65%;
  --focus-glow-bright: 200 95% 75%;

/* .theme-ocean-light */
  --focus-glow: 205 50% 50%;
  --focus-glow-bright: 205 70% 60%;

/* .theme-ocean-dark */
  --focus-glow: 205 70% 58%;
  --focus-glow-bright: 205 85% 68%;

/* .theme-forest-light */
  --focus-glow: 150 35% 38%;
  --focus-glow-bright: 150 55% 48%;

/* .theme-forest-dark */
  --focus-glow: 150 60% 52%;
  --focus-glow-bright: 150 75% 62%;

/* .theme-slate-light */
  --focus-glow: 185 60% 50%;
  --focus-glow-bright: 185 75% 60%;

/* .theme-warm-paper */
  --focus-glow: 205 55% 55%;
  --focus-glow-bright: 205 70% 65%;

/* .theme-qingye */
  --focus-glow: 340 50% 65%;
  --focus-glow-bright: 340 65% 75%;

/* .theme-black */
  --focus-glow: 43 70% 58%;
  --focus-glow-bright: 43 85% 68%;

/* .theme-the-finals */
  --focus-glow: 44 100% 62%;
  --focus-glow-bright: 44 100% 72%;

/* .theme-slate-dark */
  --focus-glow: 30 75% 62%;
  --focus-glow-bright: 30 90% 72%;
```

- [ ] **Step 2: Append the glow classes + keyframes**

At the **end** of `ui/src/styles/globals.css` (after all existing rules), append:

```css
/* ===== Focus Mode hot-zone glow indicator ===== */

.focus-glow-core {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    to bottom,
    transparent 0%,
    hsl(var(--focus-glow)) 18%,
    hsl(var(--focus-glow)) 82%,
    transparent 100%
  );
  border-radius: 2px;
  filter: blur(0.5px);
  animation: focus-glow-breathe-core 2.4s ease-in-out infinite;
}

.focus-glow-soft {
  position: absolute;
  top: 0; bottom: 0; left: -8px;
  width: 24px;
  background: linear-gradient(
    to bottom,
    transparent 0%,
    hsl(var(--focus-glow)) 25%,
    hsl(var(--focus-glow)) 75%,
    transparent 100%
  );
  filter: blur(10px);
  opacity: 0.55;
  animation: focus-glow-breathe-soft 2.4s ease-in-out infinite;
}
.focus-glow-soft-right {
  left: auto;
  right: -8px;
}

.focus-glow-halo {
  position: absolute;
  top: 0; bottom: 0; left: -16px;
  width: 56px;
  background: radial-gradient(
    ellipse at left center,
    hsl(var(--focus-glow)) 0%,
    transparent 70%
  );
  filter: blur(8px);
  opacity: 0.28;
  animation: focus-glow-breathe-halo 2.4s ease-in-out infinite;
}
.focus-glow-halo-right {
  left: auto;
  right: -16px;
  background: radial-gradient(
    ellipse at right center,
    hsl(var(--focus-glow)) 0%,
    transparent 70%
  );
}

.focus-glow-trace {
  position: absolute;
  left: -4px;
  top: 0;
  width: 12px;
  height: 80px;
  margin-top: -40px;
  background: radial-gradient(
    ellipse at left center,
    hsl(var(--focus-glow-bright)) 0%,
    transparent 65%
  );
  filter: blur(6px);
  opacity: 0.75;
  will-change: transform;
  transition: transform 0.06s linear;
}
.focus-glow-trace-right {
  left: auto;
  right: -4px;
  background: radial-gradient(
    ellipse at right center,
    hsl(var(--focus-glow-bright)) 0%,
    transparent 65%
  );
}

@keyframes focus-glow-breathe-core {
  0%, 100% { opacity: 0.85; }
  50%      { opacity: 1; }
}
@keyframes focus-glow-breathe-soft {
  0%, 100% { opacity: 0.45; }
  50%      { opacity: 0.65; }
}
@keyframes focus-glow-breathe-halo {
  0%, 100% { opacity: 0.22; }
  50%      { opacity: 0.34; }
}
```

- [ ] **Step 3: Verify tsc + tests still pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: tsc clean. No test diff (CSS only).

- [ ] **Step 4: Commit**

```bash
git branch --show-current
git add ui/src/styles/globals.css
git commit -m "feat(focus-mode): theme tokens + 3-layer glow CSS

Each of the 12 themes (root / dark / ocean-light / ocean-dark /
forest-light / forest-dark / slate-light / warm-paper / qingye / black /
the-finals / slate-dark) gets --focus-glow + --focus-glow-bright.
5 themes' values are tuned versions of their own primary; 7 themes
that have unsuitable primaries (monochrome :root / .dark, desaturated
warm-paper / slate-*, too-dim ocean-dark / forest-dark) get hue-family
substitutes — see spec § Theme色 token for the rationale per theme.

Glow CSS: 3 layers (core / soft / halo) each with a 2.4s breathing
keyframe at different amplitudes, plus a trace layer that GlowIndicator
positions imperatively via translateY().
"
```

---

## Task 7: `FloatingIsland` component

**Model:** sonnet (Framer Motion variants + click-outside detection through Radix portals)

**Files:**
- Create: `ui/src/components/focus-mode/FloatingIsland.tsx`
- Test: `ui/src/components/focus-mode/FloatingIsland.test.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write failing test**

Create `ui/src/components/focus-mode/FloatingIsland.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FloatingIsland } from './FloatingIsland'
import {
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'

describe('FloatingIsland', () => {
  it('renders children when reveal matches its side', () => {
    const { store } = renderWithProviders(
      <FloatingIsland side="left">
        <div>left-children</div>
      </FloatingIsland>,
    )
    store.set(focusRevealSideAtom, 'left')
    expect(screen.queryByText('left-children')).not.toBeNull()
  })

  it('does NOT render children when reveal is the other side', () => {
    const { store } = renderWithProviders(
      <FloatingIsland side="left">
        <div>left-children</div>
      </FloatingIsland>,
    )
    store.set(focusRevealSideAtom, 'right')
    expect(screen.queryByText('left-children')).toBeNull()
  })

  it('clicking inside the island sets pinned = true', async () => {
    const { store, user } = renderWithProviders(
      <FloatingIsland side="left">
        <button>inside-button</button>
      </FloatingIsland>,
    )
    store.set(focusRevealSideAtom, 'left')
    await user.click(screen.getByText('inside-button'))
    expect(store.get(focusRevealPinnedAtom)).toBe(true)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FloatingIsland.test.tsx 2>&1 | tail -10
```

Expected: FAIL — `Cannot find module './FloatingIsland'`.

- [ ] **Step 3: Implement FloatingIsland**

Create `ui/src/components/focus-mode/FloatingIsland.tsx`:

```tsx
/**
 * FloatingIsland — visual wrapper that animates a sidebar into / out of
 * a rounded "island" over the central preview area. The sidebar component
 * (LeftSidebar / RightSidePanel) is passed as `children` and rendered
 * unmodified — this wrapper only handles positioning + animation + the
 * click-outside-to-unpin contract.
 *
 * Click-outside detection uses a capture-phase document listener and
 * explicitly EXCLUDES Radix portal nodes ([data-radix-portal] /
 * data-radix-popper-content-wrapper / [role="dialog"]) so that
 * dropdowns, tooltips, and the global ApprovalModal can be interacted
 * with without un-pinning the island.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { AnimatePresence, motion, type Variants } from 'motion/react'
import { cn } from '@/lib/utils'
import {
  focusRevealSideAtom,
  focusRevealPinnedAtom,
} from '@/atoms/focus-mode-atoms'
import {
  ISLAND_EDGE_GAP,
  ISLAND_LEFT_WIDTH,
  ISLAND_RIGHT_WIDTH,
} from '@/lib/focus-mode-geometry'

interface Props {
  side: 'left' | 'right'
  children: React.ReactNode
}

const islandVariants: Variants = {
  hidden: (side: 'left' | 'right') => ({
    x: side === 'left' ? 'calc(-100% - 12px)' : 'calc(100% + 12px)',
    opacity: 0,
    scale: 0.96,
  }),
  shown: { x: 0, opacity: 1, scale: 1 },
}

/** Returns true if `target` is inside a Radix-managed floating overlay
 *  (portal / popper / dialog). These nodes are visually OUTSIDE the
 *  island in the DOM but are logically "inside the same interaction" —
 *  clicking them must not un-pin. */
function isInsideRadixPortal(target: Element | null): boolean {
  if (!target) return false
  return Boolean(
    target.closest('[data-radix-portal]') ||
    target.closest('[data-radix-popper-content-wrapper]') ||
    target.closest('[role="dialog"]') ||
    target.closest('[role="menu"]') ||
    target.closest('[role="tooltip"]'),
  )
}

export function FloatingIsland({ side, children }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const setPinned = useSetAtom(focusRevealPinnedAtom)
  const islandRef = React.useRef<HTMLDivElement>(null)
  const visible = reveal === side

  React.useEffect(() => {
    if (!visible) return
    const onDocClick = (e: MouseEvent) => {
      const target = e.target as Element | null
      if (isInsideRadixPortal(target)) return
      if (islandRef.current?.contains(target)) {
        setPinned(true)
      } else {
        setPinned(false)
      }
    }
    document.addEventListener('click', onDocClick, true)
    return () => document.removeEventListener('click', onDocClick, true)
  }, [visible, setPinned])

  const width = side === 'left' ? ISLAND_LEFT_WIDTH : ISLAND_RIGHT_WIDTH
  const sidePos = side === 'left' ? `left-3` : `right-3`

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          ref={islandRef}
          custom={side}
          variants={islandVariants}
          initial="hidden"
          animate="shown"
          exit="hidden"
          transition={{ duration: 0.26, ease: [0.32, 0.72, 0, 1] }}
          className={cn(
            'fixed z-[80]',
            sidePos,
            'rounded-xl bg-popover/96 backdrop-blur-md overflow-hidden',
            'shadow-[0_1px_3px_rgba(0,0,0,0.10),0_12px_36px_-8px_rgba(0,0,0,0.25),0_0_0_1px_hsl(var(--border)/0.4)]',
          )}
          style={{
            top: ISLAND_EDGE_GAP,
            bottom: ISLAND_EDGE_GAP,
            width,
          }}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  )
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FloatingIsland.test.tsx 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: 3 tests pass. tsc clean.

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/components/focus-mode/FloatingIsland.tsx ui/src/components/focus-mode/FloatingIsland.test.tsx
git commit -m "feat(focus-mode): FloatingIsland — animated island wrapper

Framer Motion variants drive the slide+scale+fade animation when the
island enters/exits. Geometry constants (top/bottom/width) come from
focus-mode-geometry.ts so they stay in lockstep with the hot zone's
bounding-box check.

Click-outside detection uses capture-phase listening + explicit
exclusion of Radix portal nodes (dropdown / tooltip / dialog / menu)
so opening a session-row context menu doesn't un-pin the island.
"
```

---

## Task 8: `GlowIndicator` component

**Model:** sonnet (imperative DOM update for Y trace performance + opacity interpolation)

**Files:**
- Create: `ui/src/components/focus-mode/GlowIndicator.tsx`
- Test: `ui/src/components/focus-mode/GlowIndicator.test.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write failing test**

Create `ui/src/components/focus-mode/GlowIndicator.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { GlowIndicator } from './GlowIndicator'

describe('GlowIndicator', () => {
  it('renders the three-layer glow + trace with the correct side classes', () => {
    renderWithProviders(<GlowIndicator side="left" />)
    const wrapper = screen.getByTestId('focus-glow-left')
    expect(wrapper).not.toBeNull()
    // Each of halo / soft / core / trace should be present
    expect(wrapper.querySelector('.focus-glow-halo')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-soft')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-core')).not.toBeNull()
    expect(wrapper.querySelector('.focus-glow-trace')).not.toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/GlowIndicator.test.tsx 2>&1 | tail -10
```

Expected: FAIL — `Cannot find module './GlowIndicator'`.

- [ ] **Step 3: Implement GlowIndicator**

Create `ui/src/components/focus-mode/GlowIndicator.tsx`:

```tsx
/**
 * GlowIndicator — soft three-layer breathing glow at one screen edge
 * during Focus Mode. The outer opacity is driven by mouse-to-edge
 * distance; the Y trace position is imperatively translated into place
 * via a ref + useEffect so we don't trigger a full React re-render on
 * every mousemove (mousemove fires ~60Hz; React reconcile + diff on
 * every event is wasted work for a pure visual effect).
 *
 * Visibility hides entirely (opacity 0) once the matching island is
 * revealed — once the island is on screen the user doesn't need an
 * additional "you can summon me" hint.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { motion } from 'motion/react'
import { cn } from '@/lib/utils'
import {
  focusRevealSideAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'

interface Props { side: 'left' | 'right' }

/** Distance at which the glow starts to brighten. */
const FADE_START_PX = 80
/** Distance at which the glow reaches full opacity. */
const FADE_PEAK_PX = 16

function proximityOpacity(dist: number): number {
  if (dist > FADE_START_PX) return 0
  if (dist < FADE_PEAK_PX) return 1
  return 1 - (dist - FADE_PEAK_PX) / (FADE_START_PX - FADE_PEAK_PX)
}

export function GlowIndicator({ side }: Props): React.ReactElement {
  const reveal = useAtomValue(focusRevealSideAtom)
  const mouse = useAtomValue(focusMousePosAtom)
  const traceRef = React.useRef<HTMLDivElement>(null)

  // Y trace: imperative DOM update, no React re-render on every mousemove.
  React.useEffect(() => {
    const el = traceRef.current
    if (!el) return
    el.style.transform = `translateY(${mouse.y}px)`
  }, [mouse.y])

  const dist = side === 'left'
    ? mouse.x
    : Math.max(0, window.innerWidth - mouse.x)
  const isRevealed = reveal === side
  const opacity = isRevealed ? 0 : proximityOpacity(dist)

  return (
    <motion.div
      aria-hidden
      data-testid={`focus-glow-${side}`}
      animate={{ opacity }}
      transition={{ duration: 0.15, ease: 'easeOut' }}
      className={cn(
        'fixed top-0 bottom-0 z-[79] pointer-events-none w-1',
        side === 'left' ? 'left-0' : 'right-0',
      )}
    >
      <div className={cn('focus-glow-halo', side === 'right' && 'focus-glow-halo-right')} />
      <div className={cn('focus-glow-soft', side === 'right' && 'focus-glow-soft-right')} />
      <div className="focus-glow-core" />
      <div
        ref={traceRef}
        className={cn('focus-glow-trace', side === 'right' && 'focus-glow-trace-right')}
      />
    </motion.div>
  )
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/GlowIndicator.test.tsx 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: 1 test passes. tsc clean.

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/components/focus-mode/GlowIndicator.tsx ui/src/components/focus-mode/GlowIndicator.test.tsx
git commit -m "feat(focus-mode): GlowIndicator — three-layer breathing edge glow

Three stacked divs (.focus-glow-halo / .focus-glow-soft /
.focus-glow-core) read hsl(var(--focus-glow)) so colour adapts to
each theme. CSS keyframes drive the 2.4s breathing.

Outer opacity interpolates from 0 (≥ 80 px away) to 1 (≤ 16 px from
edge) — gives a 'closer = brighter' summoning cue. Goes to 0 entirely
once the matching island is revealed (no need to keep advertising).

Y trace position is set imperatively in a ref-based useEffect rather
than via inline style — mousemove fires ~60 Hz and we don't want React
re-rendering the whole tree on every event.
"
```

---

## Task 9: `FocusModeOverlay` composition root

**Model:** sonnet (composition + hook mount lifecycle)

**Files:**
- Create: `ui/src/components/focus-mode/FocusModeOverlay.tsx`
- Test: `ui/src/components/focus-mode/FocusModeOverlay.test.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write failing test**

Create `ui/src/components/focus-mode/FocusModeOverlay.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FocusModeOverlay } from './FocusModeOverlay'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'

describe('FocusModeOverlay', () => {
  it('renders nothing when focus mode is OFF', () => {
    renderWithProviders(<FocusModeOverlay />)
    expect(screen.queryByTestId('focus-glow-left')).toBeNull()
    expect(screen.queryByTestId('focus-glow-right')).toBeNull()
  })

  it('renders both glow indicators when focus mode is ON and preview is open', () => {
    const { store } = renderWithProviders(<FocusModeOverlay />)
    store.set(previewPanelOpenAtom, true)
    store.set(focusModeAtom, true)
    expect(screen.queryByTestId('focus-glow-left')).not.toBeNull()
    expect(screen.queryByTestId('focus-glow-right')).not.toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FocusModeOverlay.test.tsx 2>&1 | tail -10
```

Expected: FAIL — `Cannot find module './FocusModeOverlay'`.

- [ ] **Step 3: Implement FocusModeOverlay**

Create `ui/src/components/focus-mode/FocusModeOverlay.tsx`:

```tsx
/**
 * FocusModeOverlay — composition root for Focus Mode visuals.
 *
 * Responsibilities:
 *   - Mounts the hot zone listener (useFocusModeHotzone) and the
 *     auto-exit watcher (useFocusModeAutoExit). The shortcut binding
 *     (useFocusModeShortcut) is mounted at AppShell level instead so it
 *     stays alive even when this overlay returns null.
 *   - When focus mode is OFF, returns null (nothing on screen).
 *   - When focus mode is ON, renders the two GlowIndicators and the
 *     two FloatingIsland wrappers, with LeftSidebar / RightSidePanel
 *     as the islands' children. The right island is also gated on the
 *     existing `showRightPanel` condition (agent mode + active session).
 *
 * This file is the single place that imports LeftSidebar / RightSidePanel
 * for overlay use — their original AppShell inline mounts are conditionally
 * suppressed by AppShell when focusModeAtom is true.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { previewPanelOpenAtom } from '@/atoms/preview-panel-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { useFocusModeHotzone } from '@/hooks/useFocusModeHotzone'
import { useFocusModeAutoExit } from '@/hooks/useFocusModeAutoExit'
import { LeftSidebar } from '@/components/app-shell/LeftSidebar'
import { RightSidePanel } from '@/components/app-shell/RightSidePanel'
import { FloatingIsland } from './FloatingIsland'
import { GlowIndicator } from './GlowIndicator'

export function FocusModeOverlay(): React.ReactElement | null {
  const focusMode = useAtomValue(focusModeAtom)
  const previewOpen = useAtomValue(previewPanelOpenAtom)
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const showRightPanel = appMode === 'agent' && !!currentSessionId

  // Always mount the hotzone + autoExit watchers — they cheaply no-op
  // when focusMode is false, but staying mounted means we don't miss
  // the moment focusMode flips on.
  useFocusModeHotzone()
  useFocusModeAutoExit()

  if (!focusMode || !previewOpen) return null

  return (
    <>
      <GlowIndicator side="left" />
      <GlowIndicator side="right" />
      <FloatingIsland side="left">
        <LeftSidebar />
      </FloatingIsland>
      {showRightPanel && (
        <FloatingIsland side="right">
          <RightSidePanel />
        </FloatingIsland>
      )}
    </>
  )
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FocusModeOverlay.test.tsx 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: 2 tests pass. tsc clean.

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/components/focus-mode/FocusModeOverlay.tsx ui/src/components/focus-mode/FocusModeOverlay.test.tsx
git commit -m "feat(focus-mode): FocusModeOverlay composition + watcher mounts

Renders nothing when (!focusMode || !previewOpen). When active,
renders the two GlowIndicators and the two FloatingIsland wrappers
containing the unmodified LeftSidebar / RightSidePanel.

Watchers (useFocusModeHotzone + useFocusModeAutoExit) are mounted
unconditionally so they catch the moment focusMode flips on; both
no-op cheaply when off. The Alt+F shortcut binding goes on AppShell
itself (next task), not here, so the keybind survives even when
this component returns null.
"
```

---

## Task 10: `FocusModeButton` component

**Model:** sonnet (small UI but needs proper tooltip + a11y + atom binding test)

**Files:**
- Create: `ui/src/components/focus-mode/FocusModeButton.tsx`
- Test: `ui/src/components/focus-mode/FocusModeButton.test.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Write failing test**

Create `ui/src/components/focus-mode/FocusModeButton.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FocusModeButton } from './FocusModeButton'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'

describe('FocusModeButton', () => {
  it('renders the enter-focus title when focus mode is OFF', () => {
    renderWithProviders(<FocusModeButton />)
    const btn = screen.getByRole('button', { name: /进入专注模式/ })
    expect(btn).not.toBeNull()
  })

  it('flips title to exit-focus when focus mode is ON, and click toggles the atom', async () => {
    const { store, user } = renderWithProviders(<FocusModeButton />)
    expect(store.get(focusModeAtom)).toBe(false)
    await user.click(screen.getByRole('button', { name: /进入专注模式/ }))
    expect(store.get(focusModeAtom)).toBe(true)
    // After toggle, the button's aria-label updates
    expect(screen.getByRole('button', { name: /退出专注模式/ })).not.toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FocusModeButton.test.tsx 2>&1 | tail -10
```

Expected: FAIL — `Cannot find module './FocusModeButton'`.

- [ ] **Step 3: Implement FocusModeButton**

Create `ui/src/components/focus-mode/FocusModeButton.tsx`:

```tsx
/**
 * FocusModeButton — toggles Focus Mode from the preview header.
 *
 * Mounted in PreviewHeader to the LEFT of the Copy / Reveal / Close
 * action trio. Visual style matches the existing HeaderButton:
 * size-6, rounded-md, hover bg/text tokens. Icon flips Maximize2 →
 * Minimize2 when on; tooltip / aria-label reflect the current state
 * + shortcut hint.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Maximize2, Minimize2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  focusModeAtom,
  toggleFocusModeAction,
} from '@/atoms/focus-mode-atoms'

export function FocusModeButton(): React.ReactElement {
  const focusMode = useAtomValue(focusModeAtom)
  const toggle = useSetAtom(toggleFocusModeAction)
  const label = focusMode ? '退出专注模式 (Alt+F)' : '进入专注模式 (Alt+F)'
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={() => toggle()}
      className={cn(
        'size-6 inline-flex items-center justify-center rounded-md shrink-0',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.06] active:bg-foreground/[0.10]',
      )}
    >
      {focusMode ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
    </button>
  )
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/focus-mode/FocusModeButton.test.tsx 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
```

Expected: 2 tests pass. tsc clean.

- [ ] **Step 5: Commit**

```bash
git branch --show-current
git add ui/src/components/focus-mode/FocusModeButton.tsx ui/src/components/focus-mode/FocusModeButton.test.tsx
git commit -m "feat(focus-mode): FocusModeButton — header button + atom binding

Visual style mirrors the other HeaderButton instances in
PreviewHeader (size-6, rounded-md, foreground/55 → foreground on
hover, ring-ring on focus-visible). Maximize2 / Minimize2 icons
make the current state obvious at a glance.
"
```

---

## Task 11: Integrate FocusModeButton into PreviewHeader

**Model:** sonnet (one-line import + JSX insertion, but inside an existing component with its own conventions)

**Files:**
- Modify: `ui/src/components/preview/PreviewHeader.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Add the import and the button**

In `ui/src/components/preview/PreviewHeader.tsx`:

After line 19 (current last import `import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'`), add:

```tsx
import { FocusModeButton } from '@/components/focus-mode/FocusModeButton'
```

In the JSX returned by `PreviewHeader`, find this block (currently lines 162–186):

```tsx
      {absolutePath && (
        <HeaderButton
          ariaLabel={copied ? '路径已复制' : '复制完整路径'}
          ...
```

Insert the FocusModeButton **immediately before** the `{absolutePath && ...}` Copy button:

```tsx
      <FocusModeButton />
      {absolutePath && (
        <HeaderButton
          ariaLabel={copied ? '路径已复制' : '复制完整路径'}
          title={copied ? '已复制' : '复制完整路径'}
          onClick={handleCopy}
        >
          {copied ? <Check size={13} className="text-emerald-500" /> : <Copy size={13} />}
        </HeaderButton>
      )}
```

The header's final right-side order ends up: `FocusModeButton · Copy (if path) · Reveal · Close`.

- [ ] **Step 2: Verify tsc + tests still pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: tsc clean, full suite green.

- [ ] **Step 3: Commit**

```bash
git branch --show-current
git add ui/src/components/preview/PreviewHeader.tsx
git commit -m "feat(focus-mode): mount FocusModeButton in PreviewHeader

Placed to the left of the existing Copy / Reveal / Close trio so the
button group reads left-to-right: enter-focus → copy → reveal → close.
Doesn't change any existing handler or style.
"
```

---

## Task 12: Integrate FocusModeOverlay into AppShell

**Model:** sonnet (conditional render of two existing components + mount of shortcut hook + new overlay sibling)

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx`

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current
```

- [ ] **Step 1: Add imports**

In `ui/src/components/app-shell/AppShell.tsx`, after the existing import block (around line 44 just after `import { WorkspaceTabCleaner } from './WorkspaceTabCleaner'`), add:

```tsx
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { useFocusModeShortcut } from '@/hooks/useFocusModeShortcut'
import { FocusModeOverlay } from '@/components/focus-mode/FocusModeOverlay'
```

- [ ] **Step 2: Read focusMode and mount the shortcut hook**

Inside the `AppShell` function body, near the top (after `const showRightPanel = ...` on line 56), add:

```tsx
  const focusMode = useAtomValue(focusModeAtom)
  useFocusModeShortcut()  // global Alt+F binding
```

- [ ] **Step 3: Conditionally render the inline sidebars**

The current shell body (lines 278–306) renders LeftSidebar and RightSidePanel unconditionally. Wrap each in a `!focusMode` guard. The relevant block becomes:

```tsx
      <div className="shell-bg h-screen w-screen flex overflow-hidden bg-gradient-to-br from-zinc-50 to-zinc-100 dark:from-zinc-950 dark:to-zinc-900">
        {/* 左侧边栏：focus mode 下隐藏，由 FocusModeOverlay 接管 */}
        {!focusMode && (
          <div className="sidebar-wrapper p-2 pr-0 relative z-[60]">
            <LeftSidebar />
          </div>
        )}

        {/* 中间容器：主内容区域 */}
        <div data-tauri-drag-region className="main-panel titlebar-drag-region flex-1 min-w-0 p-2 relative z-[60]">
          <div aria-hidden="true" className="main-panel-bg pointer-events-none absolute inset-0 z-0" />
          <div className="relative z-10 flex flex-col h-full min-h-0 min-w-0">
            <ModeBanner />
            <MainArea />
          </div>
        </div>

        {/* 右侧边栏：focus mode 下同样隐藏 */}
        {!focusMode && showRightPanel && (
          <div className={cn('right-panel-wrapper relative z-[60] transition-[padding] duration-300 ease-in-out', isPanelOpen ? 'p-2 pl-0' : 'p-0')}>
            <RightSidePanel />
          </div>
        )}

        {/* Global ⌘K search palette — mounts at root so it works from any view */}
        <SearchPalette onSelect={handleSearchResultSelect} />
```

- [ ] **Step 4: Mount the FocusModeOverlay sibling**

The overlay must mount OUTSIDE the conditional flex children but INSIDE the same `shell-bg` parent (or at app-shell root) so it covers the same viewport. Add it as a sibling AFTER `<SearchPalette />` and BEFORE the closing `</div>` of `shell-bg`:

```tsx
        <SearchPalette onSelect={handleSearchResultSelect} />

        {/* Focus Mode overlay — null when off, otherwise renders the two
            floating-island re-mounts of LeftSidebar / RightSidePanel + the
            two edge glow indicators. */}
        <FocusModeOverlay />

        {/* (existing) Global tool-approval modal ... */}
```

(Keep all subsequent global mounts — ApprovalModal etc. — exactly as-is.)

- [ ] **Step 5: Verify tsc + tests still pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```

Expected: tsc clean. All tests green.

- [ ] **Step 6: Commit**

```bash
git branch --show-current
git add ui/src/components/app-shell/AppShell.tsx
git commit -m "feat(focus-mode): wire Alt+F + FocusModeOverlay into AppShell

- useFocusModeShortcut() mounted at AppShell top so Alt+F is alive
  in every app state (including chat-only / settings views; the auto-
  exit watcher prevents Focus Mode from sticking when there's no
  preview to focus).
- LeftSidebar and RightSidePanel are now wrapped in {!focusMode && ...}
  guards — when Focus Mode is on, the inline flex children disappear
  entirely (MainArea fills the freed space) and the overlay component
  re-renders them as floating islands instead.
- <FocusModeOverlay /> mounts as a sibling to <SearchPalette /> so it
  shares the shell viewport but stays outside the flex layout flow.
"
```

---

## Task 13: Final verification

**Model:** sonnet (full-suite verification + end-to-end manual check)

**Files:** none — verification only.

- [ ] **Step 0: Confirm branch**

```bash
git branch --show-current   # claude/focus-mode
git log --oneline | head -15
```

Expected: 12 commits ahead of the spec commit (one per task 1–12).

- [ ] **Step 1: Run full tsc + test sweep**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -15
```

Expected: tsc clean (no errors, no warnings). Tests pass with ~387 (363 baseline + ~24 new).

- [ ] **Step 2: Rust safety check (must be NO changes)**

```bash
git status src-tauri/
```

Expected: empty / clean. If src-tauri/ has any modification, **BLOCKED** — investigate.

- [ ] **Step 3: Theme token audit**

```bash
grep -rn "#[0-9a-fA-F]\{3,8\}" ui/src/components/focus-mode/ ui/src/hooks/useFocusModeHotzone.ts ui/src/hooks/useFocusModeAutoExit.ts ui/src/hooks/useFocusModeShortcut.ts ui/src/atoms/focus-mode-atoms.ts ui/src/lib/focus-mode-geometry.ts 2>&1
```

Expected: no hits. (The CSS rgba()-in-shadow hex-equivalents inside `shadow-[...]` ARE allowed — they're alpha tints, not theme colours. Visual inspection of the diff should confirm only `hsl(var(--focus-glow))` / `hsl(var(--focus-glow-bright))` / `bg-popover` / theme tokens are used for actual UI colour decisions.)

- [ ] **Step 4: Manual smoke (run `cargo tauri dev` from src-tauri and exercise these)**

Open a markdown file in preview. Then:

1. Press `Alt+F` → LeftSidebar + RightSidePanel slide out. PreviewHeader's Focus icon flips from Maximize2 to Minimize2.
2. Move mouse to within 80 px of left edge → soft three-layer glow appears, brightens as you get closer.
3. Move to ≤ 32 px of left edge → LeftSidebar slides in as a rounded floating island.
4. Move mouse away → 200 ms later, island slides back out.
5. Re-summon, then click a session row inside the island → island stays open (pinned).
6. Click outside the island (on the preview) → island closes.
7. Right edge: same behaviour with RightSidePanel.
8. Close the preview tab → Focus Mode exits automatically; sidebars come back inline.
9. Press `Alt+F` while typing in the chat input → Mac should NOT insert ƒ; Focus Mode toggles.
10. Switch theme (e.g. theme-the-finals → theme-ocean-light → theme-qingye) while in Focus Mode → glow colour changes smoothly.

- [ ] **Step 5: Report**

Write a short summary of what you verified and any rough edges noticed (visual mismatches, animation hitches, performance issues). End with PR-ready or BLOCKED status.

No commit in this task.

---

## Out of Scope (YAGNI — do NOT add)

- Persisting focus-mode state across app restarts
- User-customizable island widths
- Hot zone "intent timer" (50 ms stay-to-trigger)
- Touch / swipe gestures (desktop app)
- Different island animation per side
- IME composition detection for Alt+F (already handled by preventDefault default)

---

## Risk Log (carried from spec)

| Risk | Implementer guidance |
|---|---|
| LeftSidebar unmount when Focus Mode toggles → scroll position loss | Acceptable; data is in atoms. Don't add scroll-restore unless reported as a real issue |
| 60 Hz mousemove → React re-render storm | GlowIndicator's Y trace is updated imperatively via ref; the rest of the tree subscribes only to focusMousePosAtom which doesn't trigger major re-renders (jotai's structural sharing handles this). If profiling shows issues during code review, propose a throttled atom write — but do NOT pre-optimize |
| Click-outside fires for Radix portals | Handled by `isInsideRadixPortal()` exclude in FloatingIsland.tsx. If a NEW Radix surface (e.g. a new dialog primitive variant) is added later, add its selector to the exclude list |

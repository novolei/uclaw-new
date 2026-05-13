# Pet Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an AI desktop pet (Astro / Clawby) to the AgentView's composer top-right corner, showing 6 animated states (idle / hover / thinking / typing / success / error) driven by agent IPC events + composer state. Pure frontend; no Rust changes.

**Architecture:** Two-layer `<img>` crossfade pattern, animated WebP with alpha (rembg + chromakey + img2webp pipeline already complete), per-character CSS `--feet-offset` for precise anchoring. State machine: a single jotai atom holds primary state (driven by Tauri events + composer focus/text); a derived display atom overlays hover on top of idle only. Settings persisted via `atomWithStorage` (no Tauri command needed).

**Tech Stack:** React 18 + TypeScript + Jotai, animated WebP (VP8L + ALPH), CSS transform/opacity transitions, Vitest + React Testing Library + jsdom. Tauri events: `chat:stream-chunk`, `chat:stream-complete`, `chat:stream-error`, `agent:stream-reset`.

**Spec source of truth:** [docs/superpowers/specs/2026-05-13-pet-widget-design.md](../specs/2026-05-13-pet-widget-design.md). Animation pacing table + signal-mapping table live there; this plan implements them.

**Honest commit count:** ~10 commits. Single PR, bisectable per CLAUDE.md PR shape.

---

## File Structure Overview

**New files (9):**

```
ui/public/pet/                                [MODIFY — add 12 webp]
├── astro-{idle,hover,thinking,typing,success,error}.webp
└── clawby-{idle,hover,thinking,typing,success,error}.webp

ui/src/atoms/
├── pet-atoms.ts                              [CREATE — state + persistence]
└── pet-atoms.test.ts                         [CREATE]

ui/src/hooks/
├── usePetStateSync.ts                        [CREATE — Tauri listeners + composer derivation]
├── usePetStateSync.test.ts                   [CREATE]
└── usePetHover.ts                            [CREATE — small handler bundle]

ui/src/components/agent/
├── PetWidget.tsx                             [CREATE — component]
├── PetWidget.css                             [CREATE — positioning + per-char offset]
└── PetWidget.test.tsx                        [CREATE]

ui/src/components/settings/
└── PetSettings.tsx                           [CREATE — toggle + char selector]
```

**Files to modify (4):**

- `ui/src/atoms/index.ts` — re-export pet-atoms
- `ui/src/atoms/agent-atoms.ts` — add `composerFocusedAtom`, `composerHasTextAtom`
- `ui/src/components/agent/AgentView.tsx` — wire composer atoms + render `<PetWidget />`
- `ui/src/App.tsx` — call `usePetStateSync()` once
- One settings entry file (TBD by Task 10 — likely `ui/src/components/settings/AppearanceSettings.tsx` or the settings index) — add `<PetSettings />` block

---

## Task 0: Verify the source assets exist

**Files:**
- Check: `.superpowers/brainstorm/41114-1778657173/videos/webp/`

- [ ] **Step 1: Confirm 12 final WebP files exist**

Run:
```bash
ls /Users/ryanliu/Documents/uclaw/.superpowers/brainstorm/41114-1778657173/videos/webp/{astro,clawby}-{idle,hover,thinking,typing,success,error}.webp | wc -l
```
Expected: `12`

If the brainstorm session directory was cleaned up, the user can regenerate via the pipeline documented in the spec's "资源生成 pipeline 回溯" section.

- [ ] **Step 2: Sanity-check file sizes and alpha**

Run:
```bash
ls -la /Users/ryanliu/Documents/uclaw/.superpowers/brainstorm/41114-1778657173/videos/webp/*.webp | awk '{print $9, $5}'
```
Expected: All files between 1.2 MB and 2.4 MB.

---

## Task 1: Copy WebP assets into the frontend build

**Files:**
- Create: `ui/public/pet/{astro,clawby}-{idle,hover,thinking,typing,success,error}.webp` (12 files)

- [ ] **Step 1: Create the destination directory**

```bash
mkdir -p /Users/ryanliu/Documents/uclaw/ui/public/pet
```

- [ ] **Step 2: Copy the 12 final WebPs**

```bash
cp /Users/ryanliu/Documents/uclaw/.superpowers/brainstorm/41114-1778657173/videos/webp/{astro,clawby}-{idle,hover,thinking,typing,success,error}.webp \
   /Users/ryanliu/Documents/uclaw/ui/public/pet/
ls /Users/ryanliu/Documents/uclaw/ui/public/pet/ | wc -l
```
Expected output: `12`

- [ ] **Step 3: Smoke-test that Vite serves them**

Run in one terminal:
```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm run dev
```
In another terminal:
```bash
curl -sI http://localhost:5173/pet/astro-idle.webp | head -3
```
Expected: `HTTP/1.1 200 OK` and `content-type: image/webp` (or similar). Stop the dev server.

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/public/pet/
git commit -m "feat(pet): add 12 alpha WebP assets for Astro and Clawby

6 states × 2 characters = 12 animated WebPs (~22 MB total).
Generated via Veo green-screen + YUV chromakey + img2webp.
Pipeline documented in docs/superpowers/specs/2026-05-13-pet-widget-design.md."
```

---

## Task 2: Pet atoms — state types + display derivation

**Files:**
- Create: `ui/src/atoms/pet-atoms.ts`
- Create: `ui/src/atoms/pet-atoms.test.ts`
- Modify: `ui/src/atoms/index.ts` — add `export * from './pet-atoms'`

- [ ] **Step 1: Write failing tests**

Create `ui/src/atoms/pet-atoms.test.ts`:

```typescript
import { createStore } from 'jotai'
import { afterEach, describe, expect, it } from 'vitest'
import {
  petCharacterAtom,
  petDisplayStateAtom,
  petEnabledAtom,
  petHoverActiveAtom,
  petPrimaryStateAtom,
} from './pet-atoms'

describe('pet-atoms', () => {
  afterEach(() => {
    localStorage.clear()
  })

  it('defaults to disabled and Astro', () => {
    const store = createStore()
    expect(store.get(petEnabledAtom)).toBe(false)
    expect(store.get(petCharacterAtom)).toBe('astro')
  })

  it('primary state defaults to idle, hover defaults off', () => {
    const store = createStore()
    expect(store.get(petPrimaryStateAtom)).toBe('idle')
    expect(store.get(petHoverActiveAtom)).toBe(false)
    expect(store.get(petDisplayStateAtom)).toBe('idle')
  })

  it('display state returns hover when primary is idle and hover active', () => {
    const store = createStore()
    store.set(petHoverActiveAtom, true)
    expect(store.get(petDisplayStateAtom)).toBe('hover')
  })

  it.each(['thinking', 'typing', 'success', 'error'] as const)(
    'hover does NOT override primary state %s',
    (primary) => {
      const store = createStore()
      store.set(petPrimaryStateAtom, primary)
      store.set(petHoverActiveAtom, true)
      expect(store.get(petDisplayStateAtom)).toBe(primary)
    },
  )

  it('hover false returns primary unchanged', () => {
    const store = createStore()
    store.set(petPrimaryStateAtom, 'thinking')
    store.set(petHoverActiveAtom, false)
    expect(store.get(petDisplayStateAtom)).toBe('thinking')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/atoms/pet-atoms.test.ts
```
Expected: All tests FAIL with "Cannot find module './pet-atoms'".

- [ ] **Step 3: Implement pet-atoms.ts**

Create `ui/src/atoms/pet-atoms.ts`:

```typescript
/**
 * Pet widget state atoms. See docs/superpowers/specs/2026-05-13-pet-widget-design.md.
 *
 * Three layers:
 *  - User preferences (persisted): petEnabledAtom, petCharacterAtom
 *  - Primary state (runtime): petPrimaryStateAtom — driven by usePetStateSync
 *  - Hover override (runtime): petHoverActiveAtom — driven by usePetHover
 *  - Display state (derived): petDisplayStateAtom — what PetWidget renders
 *
 * Hover only overrides when primary === 'idle'. Other primary states (thinking /
 * typing / success / error) are agent-critical and must not be interrupted by
 * hover.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type PetCharacter = 'astro' | 'clawby'

export type PetPrimaryState = 'idle' | 'thinking' | 'typing' | 'success' | 'error'
export type PetState = PetPrimaryState | 'hover'

export const petEnabledAtom = atomWithStorage<boolean>('pet.enabled', false)
export const petCharacterAtom = atomWithStorage<PetCharacter>('pet.character', 'astro')

export const petPrimaryStateAtom = atom<PetPrimaryState>('idle')
export const petHoverActiveAtom = atom<boolean>(false)

export const petDisplayStateAtom = atom<PetState>((get) => {
  const primary = get(petPrimaryStateAtom)
  const hovering = get(petHoverActiveAtom)
  return hovering && primary === 'idle' ? 'hover' : primary
})
```

- [ ] **Step 4: Add re-export**

Modify `ui/src/atoms/index.ts` — append:

```typescript
export * from './pet-atoms'
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/atoms/pet-atoms.test.ts
```
Expected: All 6 tests PASS.

- [ ] **Step 6: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors mentioning pet-atoms.

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/pet-atoms.ts ui/src/atoms/pet-atoms.test.ts ui/src/atoms/index.ts
git commit -m "feat(pet): pet-atoms state machine

Five atoms:
- petEnabledAtom (persisted) — opt-in toggle
- petCharacterAtom (persisted) — astro | clawby
- petPrimaryStateAtom — driven by usePetStateSync
- petHoverActiveAtom — driven by usePetHover
- petDisplayStateAtom — derived, hover only overrides idle"
```

---

## Task 3: Composer focused / has-text atoms in agent-atoms

Pet's typing state depends on whether the composer is focused AND has text. Currently `RichTextInput` in `AgentView.tsx` keeps focus + content state in local component state. Lift them to atoms.

**Files:**
- Modify: `ui/src/atoms/agent-atoms.ts` — add `composerFocusedAtom`, `composerHasTextAtom`
- Modify: `ui/src/components/agent/AgentView.tsx` — call `setComposerFocused` / `setComposerHasText` in the RichTextInput callbacks

- [ ] **Step 1: Add atoms to agent-atoms.ts**

Append to `ui/src/atoms/agent-atoms.ts` (find a logical neighbor — likely near other composer-related atoms; if none, add at the bottom under a new `// ───── composer state ─────` comment):

```typescript
// ───── composer state (lifted from RichTextInput) ─────
/** True iff the agent composer is currently focused. Lifted to atom for PetWidget. */
export const composerFocusedAtom = atom<boolean>(false)
/** True iff the agent composer's editor has non-empty text content. Lifted for PetWidget. */
export const composerHasTextAtom = atom<boolean>(false)
```

(Make sure `atom` is already imported from `jotai`; if not, add it.)

- [ ] **Step 2: Find the RichTextInput JSX in AgentView**

```bash
grep -n "RichTextInput" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx | head -5
```
Identify the JSX block where `<RichTextInput>` is rendered. Note the line numbers.

- [ ] **Step 3: Wire the atoms into RichTextInput**

In `AgentView.tsx`:

1. Add to existing imports near the top:
   ```typescript
   import { composerFocusedAtom, composerHasTextAtom } from '@/atoms/agent-atoms'
   ```
2. Inside the AgentView component, grab the setters:
   ```typescript
   const setComposerFocused = useSetAtom(composerFocusedAtom)
   const setComposerHasText = useSetAtom(composerHasTextAtom)
   ```
   (Add `useSetAtom` to the jotai import if not present.)
3. On the `<RichTextInput>` JSX, add or chain these handlers:
   ```typescript
   onFocus={() => setComposerFocused(true)}
   onBlur={() => setComposerFocused(false)}
   onChange={(value: string) => {
     setComposerHasText(value.trim().length > 0)
     // ...existing onChange logic stays
   }}
   ```
   If `RichTextInput` already has `onFocus` / `onBlur` / `onChange` props from the existing AgentView code, **wrap them** — call the existing handler then call the new setter (or vice versa). Do NOT replace existing logic; this task only adds atom sync.

- [ ] **Step 4: Run the composer reset on session change**

Find where AgentView clears the composer (typically when switching sessions or after a send). After clearing, also reset the atom:

```typescript
setComposerHasText(false)
```

Search for places where the composer text state is set to empty string in `AgentView.tsx`:
```bash
grep -nE 'setInput\(""\)|clearInput|setComposerText\(""\)' /Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx
```
At each such site, also call `setComposerHasText(false)`. (Focus state is handled by the editor's onFocus/onBlur naturally.)

- [ ] **Step 5: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No new TS errors.

- [ ] **Step 6: Smoke test in dev**

```bash
cd /Users/ryanliu/Documents/uclaw && cargo tauri dev
```
Open Agent view, type in the composer, then use React DevTools (Jotai inspector or just `console.log` temp) to verify `composerHasTextAtom` flips to `true` and back when text is cleared. Stop dev.

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/agent-atoms.ts ui/src/components/agent/AgentView.tsx
git commit -m "feat(pet): lift composer focused + has-text state to atoms

PetWidget's typing state needs to observe composer focus + text presence.
Lifted to composerFocusedAtom / composerHasTextAtom in agent-atoms.
Wired AgentView's RichTextInput onFocus / onBlur / onChange + reset sites."
```

---

## Task 4: usePetStateSync hook — Tauri events + composer derivation

**Files:**
- Create: `ui/src/hooks/usePetStateSync.ts`
- Create: `ui/src/hooks/usePetStateSync.test.ts`

- [ ] **Step 1: Write failing tests**

Create `ui/src/hooks/usePetStateSync.test.ts`:

```typescript
import { act, renderHook } from '@testing-library/react'
import { createStore, Provider } from 'jotai'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  composerFocusedAtom,
  composerHasTextAtom,
} from '@/atoms/agent-atoms'
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'

const listeners = new Map<string, (event: { payload: unknown }) => void>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (eventName: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(eventName, cb)
    return () => {
      listeners.delete(eventName)
    }
  }),
}))

import { usePetStateSync } from './usePetStateSync'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) => (
    <Provider store={store}>{children}</Provider>
  )
}

describe('usePetStateSync', () => {
  beforeEach(() => {
    listeners.clear()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('sets thinking on chat:stream-chunk', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-chunk')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')
  })

  it('sets success then auto-returns to idle after 1500ms', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-complete')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('success')
    await act(async () => {
      vi.advanceTimersByTime(1500)
    })
    expect(store.get(petPrimaryStateAtom)).toBe('idle')
  })

  it('sets error on chat:stream-error', async () => {
    const store = createStore()
    renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      listeners.get('chat:stream-error')?.({ payload: {} })
    })
    expect(store.get(petPrimaryStateAtom)).toBe('error')
  })

  it('sets typing when composer is focused with text', async () => {
    const store = createStore()
    const { rerender } = renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      store.set(composerFocusedAtom, true)
      store.set(composerHasTextAtom, true)
    })
    rerender()
    expect(store.get(petPrimaryStateAtom)).toBe('typing')
  })

  it('does not override thinking/success/error with typing', async () => {
    const store = createStore()
    store.set(petPrimaryStateAtom, 'thinking')
    const { rerender } = renderHook(() => usePetStateSync(), { wrapper: wrapper(store) })
    await act(async () => {
      store.set(composerFocusedAtom, true)
      store.set(composerHasTextAtom, true)
    })
    rerender()
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/usePetStateSync.test.ts
```
Expected: All tests FAIL with "Cannot find module './usePetStateSync'".

- [ ] **Step 3: Implement the hook**

Create `ui/src/hooks/usePetStateSync.ts`:

```typescript
/**
 * Drives petPrimaryStateAtom from Tauri agent events + composer atoms.
 * Mount this ONCE at the app root (in App.tsx). Mounting multiple times
 * registers duplicate event listeners.
 *
 * Signal mapping (see spec):
 *   chat:stream-chunk     → thinking
 *   chat:stream-complete  → success (then auto-idle after 1500ms)
 *   chat:stream-error     → error
 *   agent:stream-reset    → idle
 *   composer focused + has text → typing (only if current is not thinking/success/error)
 */
import { listen } from '@tauri-apps/api/event'
import { useAtomValue, useSetAtom } from 'jotai'
import { useEffect, useRef } from 'react'
import { composerFocusedAtom, composerHasTextAtom } from '@/atoms/agent-atoms'
import { petPrimaryStateAtom, type PetPrimaryState } from '@/atoms/pet-atoms'

const SUCCESS_LINGER_MS = 1500

export function usePetStateSync(): void {
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const focused = useAtomValue(composerFocusedAtom)
  const hasText = useAtomValue(composerHasTextAtom)
  const successTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    const unlistens: Array<() => void> = []
    listen('chat:stream-chunk', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('thinking')
    }).then((u) => unlistens.push(u))
    listen('chat:stream-complete', () => {
      setPrimary('success')
      if (successTimer.current) clearTimeout(successTimer.current)
      successTimer.current = setTimeout(() => {
        setPrimary('idle')
        successTimer.current = null
      }, SUCCESS_LINGER_MS)
    }).then((u) => unlistens.push(u))
    listen('chat:stream-error', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('error')
    }).then((u) => unlistens.push(u))
    listen('agent:stream-reset', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('idle')
    }).then((u) => unlistens.push(u))
    return () => {
      if (successTimer.current) clearTimeout(successTimer.current)
      unlistens.forEach((u) => u())
    }
  }, [setPrimary])

  // Composer-driven typing transition: only override idle (never thinking/success/error)
  useEffect(() => {
    setPrimary((prev: PetPrimaryState): PetPrimaryState => {
      if (prev === 'thinking' || prev === 'success' || prev === 'error') return prev
      return focused && hasText ? 'typing' : 'idle'
    })
  }, [focused, hasText, setPrimary])
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/hooks/usePetStateSync.test.ts
```
Expected: All 5 tests PASS.

- [ ] **Step 5: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No new errors.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/hooks/usePetStateSync.ts ui/src/hooks/usePetStateSync.test.ts
git commit -m "feat(pet): usePetStateSync — Tauri events + composer drive primary state

Listens chat:stream-chunk → thinking, chat:stream-complete → success
(auto-idle after 1500ms), chat:stream-error → error, agent:stream-reset → idle.
Composer focused + text → typing, but only when not already thinking/success/error."
```

---

## Task 5: usePetHover hook

**Files:**
- Create: `ui/src/hooks/usePetHover.ts`

- [ ] **Step 1: Implement (trivial enough to skip dedicated test — covered by PetWidget test)**

Create `ui/src/hooks/usePetHover.ts`:

```typescript
/** Returns onMouseEnter / onMouseLeave handlers that drive petHoverActiveAtom. */
import { useSetAtom } from 'jotai'
import { useCallback, useMemo } from 'react'
import { petHoverActiveAtom } from '@/atoms/pet-atoms'

export function usePetHover() {
  const setHover = useSetAtom(petHoverActiveAtom)
  const onMouseEnter = useCallback(() => setHover(true), [setHover])
  const onMouseLeave = useCallback(() => setHover(false), [setHover])
  return useMemo(() => ({ onMouseEnter, onMouseLeave }), [onMouseEnter, onMouseLeave])
}
```

- [ ] **Step 2: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/hooks/usePetHover.ts
git commit -m "feat(pet): usePetHover — mouse handlers driving petHoverActiveAtom"
```

---

## Task 6: PetWidget.css — positioning + per-char feet-offset

**Files:**
- Create: `ui/src/components/agent/PetWidget.css`

- [ ] **Step 1: Write the CSS**

Create `ui/src/components/agent/PetWidget.css`:

```css
/*
 * Positioning + per-character anchor correction.
 *
 * Anchor strategy:
 *   widget bottom = composer.top  (via position: absolute; bottom: 100%)
 *   img.transform: translateY(--feet-offset) — pushes the rendered character
 *   down within its canvas so the bottom of the visible character aligns with
 *   the widget bottom (= composer top).
 *
 * --feet-offset values measured from idle-state alpha bbox, deepest across all
 * frames (see spec §动画节奏设计 / §锚点与样式):
 *   Astro:  feet at row 673/720 → 6.39% empty below → 6.4%
 *   Clawby: paws at row 591/720 → 17.78% empty below → 17.8%
 */
.pet-widget {
  position: absolute;
  right: 16px;
  bottom: 100%;
  width: 100px;
  height: 100px;
  z-index: 2;
  pointer-events: auto;
  cursor: pointer;
}

.pet-widget .pet-layer {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
  opacity: 0;
  transition: opacity 280ms ease-in-out;
  pointer-events: none;
  transform: translateY(var(--feet-offset, 0%));
}

.pet-widget .pet-layer.active {
  opacity: 1;
}

.pet-widget[data-char="astro"] .pet-layer { --feet-offset: 6.4%; }
.pet-widget[data-char="clawby"] .pet-layer { --feet-offset: 17.8%; }

/* Tablet / mobile: scale down */
@media (max-width: 640px) {
  .pet-widget {
    width: 56px;
    height: 56px;
    right: 8px;
  }
}
```

- [ ] **Step 2: Commit (CSS only, no behavior to test yet)**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/agent/PetWidget.css
git commit -m "feat(pet): PetWidget.css — anchor + per-char feet-offset

Astro --feet-offset: 6.4%, Clawby: 17.8%. Measured from idle-state alpha bbox."
```

---

## Task 7: PetWidget component — crossfade between two img layers

**Files:**
- Create: `ui/src/components/agent/PetWidget.tsx`
- Create: `ui/src/components/agent/PetWidget.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `ui/src/components/agent/PetWidget.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createStore, Provider } from 'jotai'
import { describe, expect, it } from 'vitest'
import {
  petCharacterAtom,
  petEnabledAtom,
  petHoverActiveAtom,
  petPrimaryStateAtom,
} from '@/atoms/pet-atoms'
import { PetWidget } from './PetWidget'

function renderWith(setup: (store: ReturnType<typeof createStore>) => void = () => {}) {
  const store = createStore()
  setup(store)
  return {
    store,
    ...render(
      <Provider store={store}>
        <PetWidget data-testid="pet" />
      </Provider>,
    ),
  }
}

describe('PetWidget', () => {
  it('renders nothing when pet is disabled', () => {
    const { container } = renderWith()
    expect(container.firstChild).toBeNull()
  })

  it('renders idle img when enabled', () => {
    renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    const img = screen.getByRole('img', { hidden: true })
    expect(img.getAttribute('src')).toContain('/pet/astro-idle.webp')
  })

  it('switches character path when petCharacterAtom changes', () => {
    const { store } = renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    store.set(petCharacterAtom, 'clawby')
    const img = screen.getAllByRole('img', { hidden: true })[0]
    expect(img.getAttribute('src')).toContain('/pet/clawby-')
  })

  it('hover triggers hover state when primary is idle', async () => {
    const user = userEvent.setup()
    const { store } = renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    const widget = document.querySelector('.pet-widget') as HTMLElement
    await user.hover(widget)
    expect(store.get(petHoverActiveAtom)).toBe(true)
    await user.unhover(widget)
    expect(store.get(petHoverActiveAtom)).toBe(false)
  })

  it('renders thinking state img when primary is thinking', () => {
    renderWith((s) => {
      s.set(petEnabledAtom, true)
      s.set(petPrimaryStateAtom, 'thinking')
    })
    const imgs = screen.getAllByRole('img', { hidden: true })
    // After crossfade swap, an img with src=...-thinking.webp must exist
    const hasThinking = imgs.some((i) => i.getAttribute('src')?.includes('-thinking.webp'))
    expect(hasThinking).toBe(true)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/agent/PetWidget.test.tsx
```
Expected: All 5 tests FAIL with "Cannot find module './PetWidget'".

- [ ] **Step 3: Implement the component**

Create `ui/src/components/agent/PetWidget.tsx`:

```tsx
/**
 * AI desktop pet anchored to the AgentView composer's top-right edge.
 * Two layered <img> elements crossfade (280ms) between states.
 *
 * Animation files are animated WebP with alpha at /pet/<char>-<state>.webp.
 * State machine is driven by petPrimaryStateAtom + petHoverActiveAtom from
 * usePetStateSync + usePetHover.
 *
 * Spec: docs/superpowers/specs/2026-05-13-pet-widget-design.md
 */
import { useAtomValue } from 'jotai'
import { useEffect, useRef, useState, type HTMLAttributes } from 'react'
import { usePetHover } from '@/hooks/usePetHover'
import {
  petCharacterAtom,
  petDisplayStateAtom,
  petEnabledAtom,
  type PetState,
} from '@/atoms/pet-atoms'
import './PetWidget.css'

type Props = HTMLAttributes<HTMLDivElement>

export function PetWidget(props: Props) {
  const enabled = useAtomValue(petEnabledAtom)
  const character = useAtomValue(petCharacterAtom)
  const state = useAtomValue(petDisplayStateAtom)
  const hoverHandlers = usePetHover()

  const [activeLayer, setActiveLayer] = useState<'a' | 'b'>('a')
  const [layerAState, setLayerAState] = useState<PetState>('idle')
  const [layerBState, setLayerBState] = useState<PetState | null>(null)
  const lastShown = useRef<PetState>('idle')

  useEffect(() => {
    if (state === lastShown.current) return
    const next = activeLayer === 'a' ? 'b' : 'a'
    if (next === 'a') setLayerAState(state)
    else setLayerBState(state)
    requestAnimationFrame(() => {
      setActiveLayer(next)
      lastShown.current = state
    })
  }, [state, activeLayer])

  if (!enabled) return null

  const src = (s: PetState | null) => (s ? `/pet/${character}-${s}.webp` : '')

  return (
    <div
      {...props}
      className={`pet-widget ${props.className ?? ''}`}
      data-char={character}
      {...hoverHandlers}
    >
      <img
        className={`pet-layer ${activeLayer === 'a' ? 'active' : ''}`}
        src={src(layerAState)}
        alt=""
      />
      {layerBState !== null && (
        <img
          className={`pet-layer ${activeLayer === 'b' ? 'active' : ''}`}
          src={src(layerBState)}
          alt=""
        />
      )}
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx vitest run src/components/agent/PetWidget.test.tsx
```
Expected: All 5 tests PASS.

- [ ] **Step 5: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/agent/PetWidget.tsx ui/src/components/agent/PetWidget.test.tsx
git commit -m "feat(pet): PetWidget component with 280ms crossfade

Two-layer <img> pattern: new state src loads on inactive layer, then
requestAnimationFrame flips opacity for crossfade. Hover handlers from
usePetHover. Renders null when petEnabledAtom is false (zero cost)."
```

---

## Task 8: Render PetWidget inside AgentView composer

**Files:**
- Modify: `ui/src/components/agent/AgentView.tsx` — wrap the composer area + render `<PetWidget />`

- [ ] **Step 1: Find the composer JSX**

```bash
grep -n "RichTextInput" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx | head -3
```
The composer is the wrapper that contains `<RichTextInput>` plus any send / mic buttons. Identify its containing element.

- [ ] **Step 2: Wrap with composer-wrapper + add PetWidget**

In `AgentView.tsx`:

1. Add import at top:
   ```typescript
   import { PetWidget } from './PetWidget'
   ```
2. Wrap the composer area. The exact JSX depends on the current structure; the conceptual pattern is:
   ```tsx
   {/* Before */}
   <div className="composer-existing-classes">
     <RichTextInput ... />
     {/* mic + send buttons */}
   </div>

   {/* After */}
   <div className="composer-existing-classes composer-wrapper relative">
     <PetWidget />
     <RichTextInput ... />
     {/* mic + send buttons */}
   </div>
   ```
   The key is that the composer's outer container must be `position: relative` (Tailwind `relative` or CSS) so PetWidget's `position: absolute; bottom: 100%` anchors to it. Add `relative` class if not already present.

- [ ] **Step 3: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors.

- [ ] **Step 4: Smoke test in dev**

```bash
cd /Users/ryanliu/Documents/uclaw && cargo tauri dev
```

In the running app:
1. Open browser DevTools (Cmd+Option+I) → Console.
2. Run:
   ```js
   localStorage.setItem('pet.enabled', 'true')
   ```
3. Reload (Cmd+R). PetWidget should appear at the composer's top-right. The character should be Astro (default), idle state.
4. Type in the composer → after the text becomes non-empty, the pet should switch to typing.
5. Clear text → pet returns to idle.
6. Hover the pet → switches to hover; mouse out → back to idle.
7. Send a message → pet should go thinking while streaming, then success briefly, then idle.

Stop dev.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/agent/AgentView.tsx
git commit -m "feat(pet): render PetWidget in AgentView composer top-right

Wraps composer area in position:relative container so PetWidget anchors
correctly. Widget itself renders null when petEnabledAtom is off."
```

---

## Task 9: Mount usePetStateSync once in App.tsx

**Files:**
- Modify: `ui/src/App.tsx`

- [ ] **Step 1: Add the hook call**

In `ui/src/App.tsx`, inside the top-level component, near other hook calls:

```typescript
import { usePetStateSync } from '@/hooks/usePetStateSync'

// inside the component
usePetStateSync()
```

If there's a "global hooks" pattern (like a top-level provider that calls all global hooks), put it there instead.

- [ ] **Step 2: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors.

- [ ] **Step 3: Run full vitest suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -10
```
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/App.tsx
git commit -m "feat(pet): mount usePetStateSync once at App root"
```

---

## Task 10: PetSettings UI

**Files:**
- Create: `ui/src/components/settings/PetSettings.tsx`
- Modify: one settings page file to include `<PetSettings />`

- [ ] **Step 1: Find where settings panels live**

```bash
ls /Users/ryanliu/Documents/uclaw/ui/src/components/settings/
```
Identify the file that hosts other Appearance / Display settings. Most likely `AppearanceSettings.tsx` or a settings index. Open the chosen file and find the pattern other settings use (toggle + radio group).

- [ ] **Step 2: Implement PetSettings.tsx**

Create `ui/src/components/settings/PetSettings.tsx`:

```tsx
/**
 * Settings panel: enable desktop pet + choose character.
 * Writes to atomWithStorage atoms (petEnabledAtom / petCharacterAtom);
 * PetWidget reacts immediately.
 */
import { useAtom } from 'jotai'
import { petCharacterAtom, petEnabledAtom, type PetCharacter } from '@/atoms/pet-atoms'

const CHARACTERS: Array<{ value: PetCharacter; label: string; description: string }> = [
  { value: 'astro', label: '小宇 Astro', description: '3D 磨砂塑料宇航小子' },
  { value: 'clawby', label: '爪宝 Clawby', description: 'Tom & Jerry 风浣熊宝宝' },
]

export function PetSettings() {
  const [enabled, setEnabled] = useAtom(petEnabledAtom)
  const [character, setCharacter] = useAtom(petCharacterAtom)

  return (
    <section className="settings-section">
      <h3 className="settings-section-title">桌面宠物 (Desktop Pet)</h3>
      <p className="settings-section-desc">
        在 Agent 输入框右上角显示一个可爱的 AI 伙伴。
      </p>

      <label className="settings-toggle">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
        />
        <span>启用桌面宠物</span>
      </label>

      {enabled && (
        <div className="settings-radio-group" role="radiogroup" aria-label="选择角色">
          {CHARACTERS.map((c) => (
            <label key={c.value} className="settings-radio-option">
              <input
                type="radio"
                name="pet-character"
                value={c.value}
                checked={character === c.value}
                onChange={() => setCharacter(c.value)}
              />
              <div>
                <div className="settings-radio-label">{c.label}</div>
                <div className="settings-radio-description">{c.description}</div>
              </div>
              <img
                src={`/pet/${c.value}-idle.webp`}
                alt=""
                style={{ width: 48, height: 48, marginLeft: 'auto' }}
              />
            </label>
          ))}
        </div>
      )}
    </section>
  )
}
```

The exact class names (`settings-section`, `settings-toggle`, etc.) should match the conventions of the chosen host settings file. If that file uses Tailwind, replace these classes with the same Tailwind utilities that other settings sections use.

- [ ] **Step 3: Add `<PetSettings />` to the host settings page**

In the chosen settings file (e.g., `AppearanceSettings.tsx`):

```typescript
import { PetSettings } from './PetSettings'

// inside the JSX, in a logical location (probably after other appearance toggles)
<PetSettings />
```

- [ ] **Step 4: Smoke test**

```bash
cd /Users/ryanliu/Documents/uclaw && cargo tauri dev
```
1. Open Settings → Appearance (or wherever you placed it).
2. Toggle "启用桌面宠物" on → PetWidget should appear in AgentView.
3. Switch character → PetWidget switches to clawby.
4. Toggle off → PetWidget disappears.

Stop dev.

- [ ] **Step 5: TS check**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/settings/PetSettings.tsx <host-settings-file>
git commit -m "feat(pet): PetSettings UI — enable toggle + character selector

Writes to petEnabledAtom / petCharacterAtom (atomWithStorage). PetWidget
reacts to changes immediately via Jotai subscription."
```

---

## Task 11: Final integration verification + PR

**Files:** None (verification only)

- [ ] **Step 1: Full test suite green**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -5
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
```
Expected: All clean.

- [ ] **Step 2: Manual verification per spec §测试计划 → 集成 checklist**

Run `cargo tauri dev` and verify each item:

- [ ] 11 个主题下宠物背景全部透明、无白方块（切换 Settings → Appearance → Theme 验证 light / warm-paper / qingye / forest / forest-night / dusk 等所有主题）
- [ ] Astro 标准锚点（脚踩在 composer 顶边、不浮空、不沉入）
- [ ] Clawby 标准锚点（爪扒在 composer 顶边）
- [ ] 完整流程：聚焦 → 打字（typing 状态）→ 发送 → thinking（流式期间）→ success（1.5s）→ 回 idle
- [ ] 错误流程：发送 → thinking → 服务端错误 → 显示 error 直到下次用户输入清空 error
- [ ] Hover：idle 上能切 hover，thinking / success / error 上不打断
- [ ] Settings 切换角色：图换得过来（首次切换可能有 1-2 秒加载延迟）
- [ ] Settings 关闭：widget 消失、无资源请求（DevTools Network 验证）

- [ ] **Step 3: PR**

Push the branch and open PR. Use the bisectable-commits format:

```bash
cd /Users/ryanliu/Documents/uclaw
git push -u origin <branch-name>
gh pr create --title "feat(pet): desktop pet widget for AgentView" --body "$(cat <<'EOF'
## Summary

桌面宠物（Astro / Clawby）落在 AgentView 输入框右上角，6 个状态（idle / hover / thinking / typing / success / error）由 agent IPC 事件 + composer 焦点驱动。纯前端实现，无 Rust 改动。

## Commits (bisectable)

| Commit | Scope |
|---|---|
| feat(pet): add 12 alpha WebP assets | 资源 (ui/public/pet/) |
| feat(pet): pet-atoms state machine | atoms |
| feat(pet): lift composer focused + has-text state to atoms | atoms + AgentView wiring |
| feat(pet): usePetStateSync — Tauri events + composer drive primary state | hook |
| feat(pet): usePetHover — mouse handlers | hook |
| feat(pet): PetWidget.css — anchor + per-char feet-offset | style |
| feat(pet): PetWidget component with 280ms crossfade | component |
| feat(pet): render PetWidget in AgentView composer top-right | integration |
| feat(pet): mount usePetStateSync once at App root | integration |
| feat(pet): PetSettings UI — enable toggle + character selector | settings |

## Test plan

- [x] vitest unit + component tests pass
- [x] cargo build clean
- [x] tsc --noEmit clean
- [x] manual integration checklist in docs/superpowers/plans/2026-05-13-pet-widget.md Task 11

## Spec

[docs/superpowers/specs/2026-05-13-pet-widget-design.md](docs/superpowers/specs/2026-05-13-pet-widget-design.md)
EOF
)"
```

---

## Notes for the implementer

- **Adjacent edits that look like scope creep but aren't** (per CLAUDE.md): None for this PR. AgentView is the only composer touched; ChatInput is explicitly future scope per spec §范围外.
- **Asset path**: `/pet/<char>-<state>.webp` resolves to `ui/public/pet/<char>-<state>.webp` via Vite. Don't put assets under `src/assets/` — that triggers import processing which we don't want for binary blobs.
- **Hover only overrides idle**: this is in `petDisplayStateAtom`'s derivation, not in `usePetHover` or component logic. Don't duplicate the check anywhere else.
- **success timer**: stored in a `useRef` inside `usePetStateSync`. Cleared on stream-chunk, stream-error, stream-reset, and component unmount.
- **Composer atom resets**: when AgentView clears the composer (after send, on session switch), also reset `composerHasTextAtom` to false. Otherwise typing state lingers.
- **No backend changes**: All IPC events already exist (see Spec §状态机 → 信号映射详表). Do NOT add new Tauri commands or Rust code.
- **Font cache**: First load of each WebP costs ~1.5–2.5 MB download. Browser caches per session. Per Spec §性能, v1 does NOT preload all states; lazy loading is fine.

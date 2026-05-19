# Bottom Dock — Apex Polish · Phase 2C · Bounce on Event

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the agent needs tool-call approval, bounce the Agent dock icon to draw the user's attention. If the dock is collapsed, force-reveal it first; after the bounce + 1.5 s linger, the normal auto-hide takes over.

**Architecture:** A new `useDockBounce` hook subscribes to the existing `agent:need_approval` IPC event (already emitted by Rust; consumed today only by `ApprovalModal`). On each event, the hook (a) calls `forceReveal()` on the `BottomDockHoverRegion`, (b) increments a per-target bounce key that the matching `DockItem` reads via a new prop, and (c) schedules a 1.5 s timer that hands control back to the auto-hide debounce. `BottomDockHoverRegion` exposes `forceReveal()` + `holdRevealed(ms)` via a ref-based imperative handle. `DockItem` runs a one-shot spring-bounce keyframe whenever `bounceKey` increments — independent of the magnification spring (which freezes during the bounce window so the two animations don't compete).

**Scope (this phase only):**
- Trigger: `agent:need_approval` IPC event → bounce the Agent mode icon
- "Non-active mode receives new message" trigger is DEFERRED (no clear backend event today — Phase 2D will revisit when source UIs are wired)
- "Agent task completes (non-active mode)" trigger is OUT of scope (per spec §2.4)
- "Automation escalation" trigger is OUT of scope (per spec §2.4)
- Existing `ApprovalModal` continues to handle the actual approve/deny flow — Phase 2C only adds the *attention* layer in the dock

**Tech Stack:** React 18 + TypeScript, `@tauri-apps/api/event` `listen` (via existing `onNeedApproval`), Jotai (atom for bounce-key dict), motion/react (one-shot spring), Vitest + RTL.

**Worktree:** `.claude/worktrees/dock-apex-p2c/` on branch `claude/bottom-dock-apex-phase2c` (already created off `origin/main` at `734a474`).

**Verification cadence:**
- After every task: `cd ui && npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce.test.ts` (the last path exists only after Task 3 lands)
- After every task: `cd ui && npx tsc --noEmit 2>&1 | head` — clean for new code
- Final: `cargo tauri dev`; let the agent run a tool that needs approval (e.g. a `shell` call without a session-allow rule); the dock should auto-reveal and the Agent icon should bounce once. The `ApprovalModal` should appear as today.

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `ui/src/atoms/dock-atoms.ts` | Add `dockBounceKeysAtom: atom<Record<string, number>>` — per-target bounce counter map |
| Modify | `ui/src/atoms/dock-atoms.test.ts` | Cover bounce-keys atom + small helper |
| Create | `ui/src/hooks/useDockBounce.ts` | Hook: subscribes to `onNeedApproval`, force-reveals the dock, increments the Agent bounce key, schedules linger |
| Create | `ui/src/hooks/useDockBounce.test.ts` | Mock `onNeedApproval` + assert atom mutations + reveal/hold sequencing with fake timers |
| Modify | `ui/src/components/dock/BottomDockHoverRegion.tsx` | Expose imperative handle `{ forceReveal, holdRevealed(ms) }` via `forwardRef` |
| Modify | `ui/src/components/dock/BottomDockHoverRegion.test.tsx` | Verify the handle's forceReveal flips `data-revealed` and `holdRevealed(1500)` blocks hide for 1500 ms |
| Modify | `ui/src/components/dock/DockItem.tsx` | New optional `bounceKey?: number` prop; on increment, run a one-shot scale-1→1.35→1 keyframe (`motion.animate(...)` via `useAnimationControls` or `animate` prop change) |
| Modify | `ui/src/components/dock/DockItem.test.tsx` | Verify `bounceKey` increment triggers the animation path (via mock controls) |
| Modify | `ui/src/components/dock/BottomDock.tsx` | Read `dockBounceKeysAtom`, look up `keys[sortableId]` per item, pass to `DockItem`/`DockPinnedItem` as `bounceKey` |
| Modify | `ui/src/components/app-shell/AppShell.tsx` | Wire `useDockBounce(hoverRegionRef)` to the hover-region instance so the hook can call `forceReveal` |

Files NOT touched: `DockPinnedItem.tsx` (only modes bounce in this phase; pins can be added in 2D follow-up), `ConnectionIndicator.tsx`, `DockDragHandle.tsx`, atoms not related to dock.

---

## Task 1 · `dockBounceKeysAtom` + tests

**Files:**
- Modify: `ui/src/atoms/dock-atoms.ts`
- Modify: `ui/src/atoms/dock-atoms.test.ts`

- [ ] **Step 1: Write failing tests**

Append to `ui/src/atoms/dock-atoms.test.ts`:

```ts
import { dockBounceKeysAtom } from './dock-atoms'

describe('dockBounceKeysAtom', () => {
  it('starts empty', () => {
    const store = createStore()
    expect(store.get(dockBounceKeysAtom)).toEqual({})
  })

  it('can write a per-target bounce counter', () => {
    const store = createStore()
    store.set(dockBounceKeysAtom, (prev) => ({
      ...prev,
      'mode-agent': (prev['mode-agent'] ?? 0) + 1,
    }))
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 1 })

    store.set(dockBounceKeysAtom, (prev) => ({
      ...prev,
      'mode-agent': (prev['mode-agent'] ?? 0) + 1,
    }))
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 2 })
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c/ui
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: 2 new tests FAIL — `dockBounceKeysAtom` not exported.

- [ ] **Step 3: Implement**

Append to `ui/src/atoms/dock-atoms.ts`:

```ts
/**
 * Phase 2C bounce signal — per-target monotonic counter keyed by sortable id
 * (e.g. 'mode-agent', 'mode-chat', or future pinned-* ids). Consumers
 * (DockItem, DockPinnedItem) compare against their previous value to detect
 * "should run a one-shot bounce animation now". Resetting to 0 is unnecessary —
 * the counter is read incrementally, not absolutely.
 *
 * Intentionally NOT persisted: bounces are transient attention signals, not
 * state to remember across reloads.
 */
export const dockBounceKeysAtom = atom<Record<string, number>>({})
```

Add `atom` to the imports at the top of `dock-atoms.ts` if not already present (it should be — `dockOrderAtom` uses `atomWithStorage` which is a different export; `internetOnlineAtom` etc. use `atom`).

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: 24 PASS (22 existing + 2 new).

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep dock-atoms | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git add ui/src/atoms/dock-atoms.ts ui/src/atoms/dock-atoms.test.ts
git commit -m "feat(dock): dockBounceKeysAtom — per-target bounce counter

Phase 2C signal. atom<Record<string, number>> keyed by sortable id.
DockItem/DockPinnedItem will compare against previous value to detect
'should run one-shot bounce now'. Intentionally not persisted —
bounces are transient attention signals.

Phase 2C task 1 of 5."
```

---

## Task 2 · `BottomDockHoverRegion` imperative handle

Expose `forceReveal()` + `holdRevealed(ms)` via a ref so external hooks (Phase 2C `useDockBounce`, future Phase 3 liveness) can drive the reveal/hold timing without going through hover events.

**Files:**
- Modify: `ui/src/components/dock/BottomDockHoverRegion.tsx`
- Modify: `ui/src/components/dock/BottomDockHoverRegion.test.tsx`

- [ ] **Step 1: Write failing tests**

Append to `BottomDockHoverRegion.test.tsx`:

```tsx
import * as React from 'react'
import { act } from '@testing-library/react'
import type { BottomDockHoverRegionHandle } from './BottomDockHoverRegion'

// ...inside the existing describe('BottomDockHoverRegion', …) block...

  it('exposes forceReveal() and holdRevealed(ms) via ref', () => {
    const ref = React.createRef<BottomDockHoverRegionHandle>()
    const { container } = render(
      <JotaiProvider store={(() => {
        const s = createStore()
        s.set(bottomDockEnabledAtom, true)
        return s
      })()}>
        <BottomDockHoverRegion ref={ref} />
      </JotaiProvider>,
    )

    const region = container.querySelector('[data-revealed]') as HTMLElement
    expect(region.dataset.revealed).toBe('false')

    act(() => {
      ref.current?.forceReveal()
    })
    expect(region.dataset.revealed).toBe('true')
  })

  it('holdRevealed(ms) blocks the normal hide debounce for the duration', () => {
    vi.useFakeTimers()
    const ref = React.createRef<BottomDockHoverRegionHandle>()
    const { container } = render(
      <JotaiProvider store={(() => {
        const s = createStore()
        s.set(bottomDockEnabledAtom, true)
        return s
      })()}>
        <BottomDockHoverRegion ref={ref} />
      </JotaiProvider>,
    )
    const region = container.querySelector('[data-revealed]') as HTMLElement

    act(() => { ref.current?.forceReveal() })
    expect(region.dataset.revealed).toBe('true')

    // Start the hold — covers 1500 ms.
    act(() => { ref.current?.holdRevealed(1500) })

    // A normal mouseLeave during the hold window should NOT trigger hide.
    act(() => {
      region.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }))
      vi.advanceTimersByTime(1000) // < 1500 hold
    })
    expect(region.dataset.revealed).toBe('true')

    // After the hold elapses, hide proceeds.
    act(() => { vi.advanceTimersByTime(800) }) // total > 1500 + debounce
    expect(region.dataset.revealed).toBe('false')

    vi.useRealTimers()
  })
```

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/components/dock/BottomDockHoverRegion.test.tsx
```

Expected: TypeScript errors (`BottomDockHoverRegionHandle` not exported, ref prop not accepted).

- [ ] **Step 3: Implement — convert to forwardRef + handle**

Edit `ui/src/components/dock/BottomDockHoverRegion.tsx`. Replace the existing function declaration with a `forwardRef` wrapper.

Current signature is `export function BottomDockHoverRegion(): React.ReactElement`. New signature:

```tsx
import * as React from 'react'
import { BottomDock } from './BottomDock'

const REVEAL_HIDE_DELAY_MS = 220
const HIDE_ANIM_DURATION_MS = 460
const TRIGGER_WIDTH_PX = 440
const TRIGGER_HEIGHT_PX = 6
const REVEAL_PAD_X_PX = 24
const REVEAL_PAD_TOP_PX = 12

export interface BottomDockHoverRegionHandle {
  /**
   * Force the dock into the revealed state. Used by external attention
   * signals (Phase 2C bounce, Phase 3 future liveness). The normal
   * mouseLeave debounce will hide it again unless holdRevealed() is also
   * called.
   */
  forceReveal: () => void
  /**
   * Suppress the auto-hide debounce for `ms` after the call. If a
   * mouseLeave fires during the hold window, the hide is deferred until
   * the window expires (after which the normal debounce takes over).
   */
  holdRevealed: (ms: number) => void
}

export const BottomDockHoverRegion = React.forwardRef<
  BottomDockHoverRegionHandle,
  Record<string, never>
>(function BottomDockHoverRegion(_props, ref): React.ReactElement {
  const [revealed, setRevealed] = React.useState(false)
  const [containerOpen, setContainerOpen] = React.useState(false)
  const hideTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)
  const collapseTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)
  const holdUntilRef = React.useRef<number>(0)

  const cancelHide = React.useCallback(() => {
    if (hideTimerRef.current !== null) {
      clearTimeout(hideTimerRef.current)
      hideTimerRef.current = null
    }
    if (collapseTimerRef.current !== null) {
      clearTimeout(collapseTimerRef.current)
      collapseTimerRef.current = null
    }
  }, [])

  const scheduleHide = React.useCallback(() => {
    cancelHide()
    const now = Date.now()
    const wait = Math.max(REVEAL_HIDE_DELAY_MS, holdUntilRef.current - now)
    hideTimerRef.current = setTimeout(() => {
      setRevealed(false)
      collapseTimerRef.current = setTimeout(
        () => setContainerOpen(false),
        HIDE_ANIM_DURATION_MS,
      )
    }, wait)
  }, [cancelHide])

  const handleEnter = React.useCallback(() => {
    cancelHide()
    setRevealed(true)
    setContainerOpen(true)
  }, [cancelHide])

  React.useImperativeHandle(
    ref,
    () => ({
      forceReveal: () => {
        cancelHide()
        setRevealed(true)
        setContainerOpen(true)
      },
      holdRevealed: (ms: number) => {
        holdUntilRef.current = Date.now() + ms
      },
    }),
    [cancelHide],
  )

  React.useEffect(() => () => cancelHide(), [cancelHide])

  return (
    <div
      className="fixed bottom-0 left-1/2 z-[70] flex justify-center pointer-events-auto"
      style={{
        transform: 'translateX(-50%)',
        width: containerOpen ? 'auto' : TRIGGER_WIDTH_PX,
        height: containerOpen ? 'auto' : TRIGGER_HEIGHT_PX,
        paddingLeft: containerOpen ? REVEAL_PAD_X_PX : 0,
        paddingRight: containerOpen ? REVEAL_PAD_X_PX : 0,
        paddingTop: containerOpen ? REVEAL_PAD_TOP_PX : 0,
      }}
      onMouseEnter={handleEnter}
      onMouseLeave={scheduleHide}
      data-revealed={revealed}
      data-container-open={containerOpen}
    >
      <BottomDock revealed={revealed} />
    </div>
  )
})

BottomDockHoverRegion.displayName = 'BottomDockHoverRegion'
```

Key changes vs the current implementation:
- Wrapped in `React.forwardRef<BottomDockHoverRegionHandle, …>`
- New `holdUntilRef` — timestamp until which auto-hide is suppressed
- `scheduleHide` now picks `max(REVEAL_HIDE_DELAY_MS, holdUntilRef - now)` so an active hold defers the timer
- `useImperativeHandle` exposes `forceReveal` + `holdRevealed`
- New exported type `BottomDockHoverRegionHandle`

The function takes a `Record<string, never>` props type so that forwardRef compiles cleanly with no props.

- [ ] **Step 4: Update `AppShell.tsx` to pass a ref through (later wired in Task 5; for now just compile-clean)**

In `ui/src/components/app-shell/AppShell.tsx`, find where `<BottomDockHoverRegion />` is rendered. Wrap it with a ref:

```tsx
const dockHoverRef = React.useRef<BottomDockHoverRegionHandle>(null)
// ...
<BottomDockHoverRegion ref={dockHoverRef} />
```

Import the type if needed:

```tsx
import { BottomDockHoverRegion, type BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'
```

Task 5 will pass `dockHoverRef` into `useDockBounce`. For now, the ref is unused — that's fine; TypeScript permits unused refs.

- [ ] **Step 5: Run, expect PASS**

```bash
npm test -- --run src/components/dock/BottomDockHoverRegion.test.tsx
```

Expected: 6 tests PASS (4 existing + 2 new).

- [ ] **Step 6: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(BottomDockHoverRegion|AppShell)" | head
```

Expected: empty.

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git add ui/src/components/dock/BottomDockHoverRegion.tsx ui/src/components/dock/BottomDockHoverRegion.test.tsx ui/src/components/app-shell/AppShell.tsx
git commit -m "feat(dock): expose forceReveal + holdRevealed via imperative handle

BottomDockHoverRegion now accepts a ref of type
BottomDockHoverRegionHandle exposing two methods:
  - forceReveal(): flip to revealed state (skips the hover debounce)
  - holdRevealed(ms): suppress auto-hide for ms after the call

Internally a holdUntilRef tracks the suppression timestamp. The
scheduleHide path picks max(debounce, holdUntil - now) so an active
hold defers the timer instead of disabling it.

AppShell now mounts the hover region with a ref. The ref is
unused this commit; Task 5 wires it to useDockBounce.

Phase 2C task 2 of 5."
```

---

## Task 3 · `useDockBounce` hook

Subscribe to the existing `agent:need_approval` IPC event. On each event: increment the Agent's bounce key, call `forceReveal()` on the hover region, and call `holdRevealed(1500)` to keep the dock visible long enough for the user to register the attention signal.

**Files:**
- Create: `ui/src/hooks/useDockBounce.ts`
- Create: `ui/src/hooks/useDockBounce.test.ts`

- [ ] **Step 1: Write failing test**

Create `ui/src/hooks/useDockBounce.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import * as React from 'react'
import { useDockBounce } from './useDockBounce'
import { dockBounceKeysAtom } from '@/atoms/dock-atoms'
import type { BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'

// Capture the latest onNeedApproval callback so tests can drive it.
let needApprovalCb: ((p: unknown) => void) | null = null

vi.mock('@/lib/tauri-bridge', () => ({
  onNeedApproval: (cb: (p: unknown) => void) => {
    needApprovalCb = cb
    return Promise.resolve(() => {
      needApprovalCb = null
    })
  },
}))

describe('useDockBounce', () => {
  beforeEach(() => {
    needApprovalCb = null
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  function setup() {
    const store = createStore()
    const forceReveal = vi.fn()
    const holdRevealed = vi.fn()
    const ref: React.RefObject<BottomDockHoverRegionHandle> = {
      current: { forceReveal, holdRevealed },
    }
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <JotaiProvider store={store}>{children}</JotaiProvider>
    )
    renderHook(() => useDockBounce(ref), { wrapper })
    return { store, forceReveal, holdRevealed }
  }

  it('subscribes to onNeedApproval on mount', async () => {
    setup()
    // microtask drain — the Promise<UnlistenFn> resolves and assigns cb
    await Promise.resolve()
    expect(needApprovalCb).not.toBeNull()
  })

  it('on approval event: forceReveal + holdRevealed(1500) + bumps mode-agent counter', async () => {
    const { store, forceReveal, holdRevealed } = setup()
    await Promise.resolve()
    act(() => {
      needApprovalCb?.({ id: 'req-1', tool_name: 'shell', params: {} })
    })
    expect(forceReveal).toHaveBeenCalledTimes(1)
    expect(holdRevealed).toHaveBeenCalledWith(1500)
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 1 })
  })

  it('multiple events accumulate the counter', async () => {
    const { store } = setup()
    await Promise.resolve()
    act(() => { needApprovalCb?.({ id: 'a' }) })
    act(() => { needApprovalCb?.({ id: 'b' }) })
    act(() => { needApprovalCb?.({ id: 'c' }) })
    expect(store.get(dockBounceKeysAtom)).toEqual({ 'mode-agent': 3 })
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/hooks/useDockBounce.test.ts
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `ui/src/hooks/useDockBounce.ts`:

```ts
import { useEffect, type RefObject } from 'react'
import { useSetAtom } from 'jotai'
import { dockBounceKeysAtom } from '@/atoms/dock-atoms'
import { onNeedApproval } from '@/lib/tauri-bridge'
import type { BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'

/**
 * Attention-signal hook for the BottomDock.
 *
 * Subscribes to `agent:need_approval` IPC events; on each event:
 *   1. Calls `forceReveal()` on the hover region (slides the dock up if hidden)
 *   2. Calls `holdRevealed(1500)` so the dock stays visible for ~1.5 s
 *   3. Increments `dockBounceKeysAtom['mode-agent']` so the Agent icon
 *      runs its one-shot bounce animation (DockItem listens via bounceKey)
 *
 * After the hold expires, the normal mouseLeave debounce takes over.
 *
 * Phase 2C scope: tool-approval only. The "non-active mode new message"
 * trigger from spec §2.4 is deferred — there is no clear backend event
 * for it today. The hook is structured so additional event subscriptions
 * can plug in alongside without refactor.
 */
export function useDockBounce(
  hoverRef: RefObject<BottomDockHoverRegionHandle>,
): void {
  const setBounceKeys = useSetAtom(dockBounceKeysAtom)

  useEffect(() => {
    let unlisten: (() => void) | null = null
    let active = true

    onNeedApproval(() => {
      if (!active) return
      hoverRef.current?.forceReveal()
      hoverRef.current?.holdRevealed(1500)
      setBounceKeys((prev) => ({
        ...prev,
        'mode-agent': (prev['mode-agent'] ?? 0) + 1,
      }))
    }).then((fn) => {
      if (active) unlisten = fn
      else fn() // mount race — unlisten immediately
    })

    return () => {
      active = false
      if (unlisten) unlisten()
    }
  }, [hoverRef, setBounceKeys])
}
```

The mount race is the `listen()` returns a `Promise<UnlistenFn>`; if the component unmounts before the promise resolves, we hold a flag and unlisten in the resolution branch.

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/hooks/useDockBounce.test.ts
```

Expected: 3 tests PASS.

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(useDockBounce|dockBounceKeysAtom)" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git add ui/src/hooks/useDockBounce.ts ui/src/hooks/useDockBounce.test.ts
git commit -m "feat(dock): useDockBounce — subscribe agent:need_approval → bounce Agent

Phase 2C attention-signal hook. On each agent:need_approval event:
  1. forceReveal() the hover region (slides dock up if hidden)
  2. holdRevealed(1500) keeps it visible ~1.5 s
  3. increment dockBounceKeysAtom['mode-agent'] for the one-shot
     bounce animation on the Agent icon

Mount-race safe: unlisten promise tracked + flag to abort if
component unmounts before promise resolves.

Phase 2C scope: tool-approval only. Other triggers from spec §2.4
(non-active mode new message, agent task complete, automation
escalation) are deferred.

Phase 2C task 3 of 5."
```

---

## Task 4 · `DockItem` bounce animation

When `bounceKey` prop increments, run a one-shot scale-1→1.35→1 spring. The magnification spring stays frozen during the bounce so the two animations don't compete.

**Files:**
- Modify: `ui/src/components/dock/DockItem.tsx`
- Modify: `ui/src/components/dock/DockItem.test.tsx`

- [ ] **Step 1: Write failing test**

Append to `DockItem.test.tsx`:

```tsx
  it('runs a one-shot bounce animation when bounceKey increments', () => {
    const { rerender, container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        bounceKey={0}
      />,
    )
    const btn = screen.getByRole('button', { name: 'Agent' })
    // No bounce indicator at baseline.
    expect(btn.getAttribute('data-bouncing')).toBeNull()

    rerender(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        bounceKey={1}
      />,
    )
    // After bounceKey bump, the data-bouncing attr is set for the bounce window.
    expect(btn.getAttribute('data-bouncing')).toBe('true')

    // Container still rendered cleanly.
    expect(container).toBeTruthy()
  })
```

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/components/dock/DockItem.test.tsx
```

Expected: 1 test FAIL — `bounceKey` prop doesn't exist; `data-bouncing` not emitted.

- [ ] **Step 3: Implement**

Edit `ui/src/components/dock/DockItem.tsx`. Add `bounceKey?: number` to the props interface:

```tsx
interface DockItemProps {
  icon: React.ReactNode
  label: string
  isActive: boolean
  index: number
  hoveredIndex: number | null
  onHoverIndexChange: (index: number | null) => void
  onClick: () => void
  sortableId?: string
  /** Phase 2C: increments to trigger a one-shot bounce. */
  bounceKey?: number
}
```

Destructure it in the function signature. Add a bounce-active state + effect-on-increment:

```tsx
// Inside the function, after the existing useSortable / magnification setup:
const [bouncing, setBouncing] = React.useState(false)
const lastBounceKeyRef = React.useRef(bounceKey ?? 0)

React.useEffect(() => {
  const current = bounceKey ?? 0
  if (current > lastBounceKeyRef.current) {
    lastBounceKeyRef.current = current
    setBouncing(true)
    const t = setTimeout(() => setBouncing(false), 520) // 500 ms spring + small buffer
    return () => clearTimeout(t)
  }
  // bounceKey decreased or unchanged — no-op.
  return undefined
}, [bounceKey])
```

Pass `data-bouncing` to the `motion.button` and compose the bounce scale into the `motionStyle`:

```tsx
<motion.button
  ref={sortableId ? sortable.setNodeRef : undefined}
  type="button"
  data-sortable-id={sortableId ?? undefined}
  data-dragging={sortable.isDragging ? 'true' : undefined}
  data-bouncing={bouncing ? 'true' : undefined}
  ...
>
```

For the bounce visual itself: when `bouncing` is true, override the spring scale via a one-shot animation. The cleanest way is to add a separate motion `animate` value that takes over during the bounce window. Use `useAnimationControls`:

```tsx
import { motion, useSpring, useReducedMotion, useAnimationControls } from 'motion/react'
// ...
const bounceControls = useAnimationControls()

React.useEffect(() => {
  const current = bounceKey ?? 0
  if (current > lastBounceKeyRef.current) {
    lastBounceKeyRef.current = current
    setBouncing(true)
    bounceControls.start({
      scale: [1, 1.35, 1],
      transition: { duration: 0.5, times: [0, 0.4, 1], ease: 'easeInOut' },
    })
    const t = setTimeout(() => setBouncing(false), 520)
    return () => clearTimeout(t)
  }
  return undefined
}, [bounceKey, bounceControls])
```

Then change the `motion.button` to accept `animate={bouncing ? bounceControls : undefined}` — but the existing `motion.button` already uses `style={{ scale: ... }}` for the spring. Mixing `animate.scale` and `style.scale` is problematic.

**Cleaner approach**: keep the existing `motionStyle` for magnification + drag. Wrap the icon `<span>` in a NEW `motion.div` that handles ONLY the bounce. Bounce scales the inner content, not the entire button, which means the button's hover hitbox doesn't move and the magnification continues to drive the icon position independently.

Update the JSX inside `motion.button`:

```tsx
<motion.div
  animate={bouncing ? { scale: [1, 1.35, 1] } : { scale: 1 }}
  transition={
    bouncing
      ? { duration: 0.5, times: [0, 0.4, 1], ease: 'easeInOut' }
      : { duration: 0 }
  }
  className="flex items-center justify-center"
  style={{ width: ICON_BOX, height: ICON_BOX, transformOrigin: 'center' }}
>
  {icon}
</motion.div>
```

(Replace the existing icon-wrapper `<span>` with this `motion.div`.)

The bounce now scales the icon contents inside the button. The button's outer magnification spring is unaffected; the active dot below stays in place because it's a sibling of the icon `<span>` (now `motion.div`).

Remove the `useAnimationControls` + `bounceControls.start(...)` from the effect — we're using declarative `animate={...}` instead.

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/components/dock/DockItem.test.tsx
```

Expected: all DockItem tests PASS including the new bounce test.

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep DockItem | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git add ui/src/components/dock/DockItem.tsx ui/src/components/dock/DockItem.test.tsx
git commit -m "feat(dock): DockItem one-shot bounce on bounceKey increment

New optional bounceKey?: number prop. When it increments (compared to
previous value via lastBounceKeyRef), the icon contents run a one-shot
scale-1→1.35→1 motion over 500 ms (easeInOut, peak at 40%).

The bounce wraps the icon visual in a new motion.div sibling of the
active-dot span. The outer button's magnification spring + drag
composition are untouched — bounce and magnification ride on different
elements so they don't compete.

data-bouncing='true' is set during the bounce window for test
addressability + future styling hooks.

Phase 2C task 4 of 5."
```

---

## Task 5 · Wire useDockBounce in AppShell + thread bounceKey through BottomDock

Mount `useDockBounce(dockHoverRef)` in `AppShell`. In `BottomDock`, read `dockBounceKeysAtom` and pass `keys[sortableId]` as `bounceKey` to each `DockItem`/`DockPinnedItem`.

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx`
- Modify: `ui/src/components/dock/BottomDock.tsx`

- [ ] **Step 1: Wire AppShell**

Find the `BottomDockHoverRegion` mount in `AppShell.tsx`. After the `dockHoverRef = React.useRef(...)` from Task 2, add the hook call:

```tsx
import { useDockBounce } from '@/hooks/useDockBounce'
// ...inside AppShell function...
const dockHoverRef = React.useRef<BottomDockHoverRegionHandle>(null)
useDockBounce(dockHoverRef)
// ...
<BottomDockHoverRegion ref={dockHoverRef} />
```

- [ ] **Step 2: Wire BottomDock**

Edit `ui/src/components/dock/BottomDock.tsx`. Add the atom import:

```tsx
import { bottomDockEnabledAtom, dockOrderAtom, applyDockReorder, dockBounceKeysAtom, type DockItemSpec } from '@/atoms/dock-atoms'
```

Inside the BottomDock function, after the existing atom reads:

```tsx
const bounceKeys = useAtomValue(dockBounceKeysAtom)
```

In the render switch (Task 3 of Phase 2B), pass `bounceKey={bounceKeys[sortableId]}` to each `DockItem` and `DockPinnedItem` call.

For the `mode` case in `BottomDock`'s switch, update:

```tsx
case 'mode': {
  const meta = MODE_REGISTRY[spec.mode]
  body = (
    <DockItem
      key={sortableId}
      sortableId={sortableId}
      bounceKey={bounceKeys[sortableId]}
      icon={/* ... */}
      label={meta.label}
      isActive={meta.isActive(navCtx)}
      index={i}
      hoveredIndex={hoveredIndex}
      onHoverIndexChange={setHoveredIndex}
      onClick={() => meta.onClick(actionCtx)}
    />
  )
  break
}
```

(Add `bounceKey={bounceKeys[sortableId]}` to the DockItem props.)

The pinned-* cases in the switch don't need the bounce prop yet (no bounce trigger targets them in 2C scope) — but if the implementer wants to add the prop anyway for symmetry with DockPinnedItem's potential future use, that's fine. For minimum 2C scope, only the mode case needs the prop.

- [ ] **Step 3: Run all dock + hook tests**

```bash
npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce.test.ts
```

Expected: all PASS.

- [ ] **Step 4: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(BottomDock|AppShell|useDockBounce)" | head
```

Expected: empty.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git add ui/src/components/app-shell/AppShell.tsx ui/src/components/dock/BottomDock.tsx
git commit -m "feat(dock): wire useDockBounce in AppShell + thread bounceKey

AppShell mounts useDockBounce(dockHoverRef) so the hook can drive
forceReveal + holdRevealed when the agent:need_approval event fires.

BottomDock reads dockBounceKeysAtom and passes
bounceKey={bounceKeys[sortableId]} to each DockItem. When the agent
needs approval, the hook increments keys['mode-agent']; the Agent
icon picks up the new value via the prop and runs its one-shot
bounce.

End-to-end: agent emits need_approval → useDockBounce sees event →
forceReveal slides dock up → bounce key++ → DockItem animates →
1.5 s linger via holdRevealed → normal auto-hide takes over.

Phase 2C task 5 of 5."
```

---

## Task 6 · Verification + PR

- [ ] **Step 1: Full vitest**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c/ui
npm test -- --run 2>&1 | tail -6
```

Expected: ~880 pass / 10 baseline fail.

- [ ] **Step 2: TypeScript**

```bash
npx tsc --noEmit 2>&1 | grep -v "useBrowserScreencast\|useBrowserTaskEvents" | head -5
```

Expected: empty.

- [ ] **Step 3: Manual smoke test**

```bash
cd ../src-tauri && cargo tauri dev
```

In the dev runtime:
1. Enable the dock toggle in Settings.
2. With the dock collapsed (default), open an agent session and ask it to do something requiring tool approval (e.g. ask it to run `ls`).
3. When the approval popup appears: the dock should auto-reveal, the Agent icon should bounce once, the existing ApprovalModal should show on top.
4. Approve or deny — the dock should auto-hide ~1.5 s after the bounce.

- [ ] **Step 4: Push + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2c
git fetch origin main
git status -sb  # If behind, rebase first.
git push -u origin claude/bottom-dock-apex-phase2c
```

Open the PR:

```bash
gh pr create --base main --head claude/bottom-dock-apex-phase2c \
  --title "feat(dock): apex polish Phase 2C — bounce on tool-approval" \
  --body "$(cat <<'EOF'
## Summary

Phase 2C of the BottomDock apex polish series — the attention-signal layer. Last of three Phase-2 sub-PRs (2A reorder ✅ → 2B pin render ✅ → 2C bounce).

- **\`useDockBounce\`** (new hook) subscribes to the existing \`agent:need_approval\` IPC event (already emitted by Rust, consumed today only by \`ApprovalModal\`). On each event: force-reveal the dock, increment the Agent's bounce key, hold the dock visible for 1.5 s.
- **\`BottomDockHoverRegion\`** now exposes an imperative handle (\`forceReveal\`, \`holdRevealed(ms)\`) via \`forwardRef\`. Internally, \`scheduleHide\` picks \`max(debounce, holdUntil - now)\` so an active hold defers the auto-hide timer.
- **\`DockItem\`** accepts an optional \`bounceKey?: number\` prop. When the key increments, the icon contents run a one-shot scale-1→1.35→1 keyframe over 500 ms. The bounce wraps the icon visual in a new \`motion.div\` sibling of the active-dot span — outer button magnification + drag composition are untouched (separate elements, no animation conflict).
- **\`dockBounceKeysAtom\`** (new) is a transient \`Record<string, number>\` keyed by sortable id. Not persisted (bounces are signals, not state).

Spec: \`docs/superpowers/specs/2026-05-19-bottom-dock-apex-design.md\` §2.4
Plan: \`docs/superpowers/plans/2026-05-19-bottom-dock-apex-phase2c-bounce.md\`

## Out of scope (deferred)

- **"Non-active mode receives new message" trigger**: no clear backend event source today. Will revisit in Phase 2D when source UIs are wired (the same surfaces have message events that can drive bounce).
- **Agent task complete / Automation escalation**: per spec §2.4 already deferred.

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1 | dockBounceKeysAtom | transient counter atom + tests |
| 2 | BottomDockHoverRegion imperative handle | forceReveal + holdRevealed via forwardRef |
| 3 | useDockBounce hook | subscribes need_approval → bumps key + reveals |
| 4 | DockItem one-shot bounce animation | motion.div wrapping the icon, scale 1→1.35→1 |
| 5 | wire AppShell + thread bounceKey through BottomDock | end-to-end |

## Test plan

- [x] \`npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce.test.ts\` — full coverage passes
- [x] \`npx tsc --noEmit\` clean
- [ ] Manual: trigger tool-approval, confirm dock auto-reveals + Agent icon bounces + 1.5 s linger + auto-hides
- [ ] Manual: theme cycle — bounce visual still smooth (the bounce is opacity/scale only, no theme tokens)
EOF
)"
```

---

## Out of scope (Phase 3 reminder)

- Agent breathing ring (Phase 3)
- Streaming particles (Phase 3)
- Memory pulse (Phase 3)
- IPC event for "non-active mode new message" (Phase 2D or backend follow-up)

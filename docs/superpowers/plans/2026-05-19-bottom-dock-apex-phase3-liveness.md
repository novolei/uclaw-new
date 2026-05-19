# Bottom Dock — Apex Polish · Phase 3 · Liveness

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the dock is visible, the Agent icon **breathes** (soft halo, 2s loop) and emits **streaming particles** while the LLM is producing tokens; the Memory icon **pulses** (subtle scale wave) while memU is consolidating. When the dock is hidden, none of this renders — per spec §3.1, no edge indicator, no peek, nothing.

**Architecture:** Phase 3 is presentational + one new transient atom. A new `useDockLiveness()` hook composes the existing `agentStreamingAtom` (mid-stream signal) with a new `memuConsolidatingAtom` (default `false`; Rust-side memU bridge event drives it in a follow-up commit or follow-up phase) and returns a `Record<sortableId, LivenessState>` mapping. `DockItem` accepts an optional `liveness?: LivenessState` prop and renders three independent visuals: a halo `motion.div` behind the icon (breathing), particle children spawned at 400 ms intervals (streaming), and a continuous scale-pulse animation on the icon wrapper (memory only — composes with the existing bounce animation by branching the `animate` value).

**Signal source caveats:**
- Spec §3.2 distinguishes "breathing ring" (broader `activeTasks.length > 0`) from "streaming particles" (`agentStreamingAtom`). The codebase has no `activeTasks` concept today — only `agentStreamingAtom`. Phase 3 uses `agentStreamingAtom` for BOTH visuals (they layer rather than compete: the halo is ambient, the particles are the active output). A future refactor can split if a "broader activity" atom is introduced.
- `memuConsolidatingAtom` is added with a `false` default. The Rust-side memU bridge does not currently emit `memu_consolidation_started` / `_finished` events; the atom stays `false` and the Memory pulse never plays. This is graceful degradation (per spec §3.3). A follow-up PR can add the Rust events.

**Tech Stack:** React 18 + TypeScript, Jotai, motion/react (existing), Vitest + RTL.

**Worktree:** `.claude/worktrees/dock-apex-p3/` on branch `claude/bottom-dock-apex-phase3` (already created off `origin/main` at `8fc88f6`).

**Verification cadence:**
- After every task: `cd ui && npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce src/hooks/useDockLiveness` (the last path exists only after Task 2)
- After every task: `cd ui && npx tsc --noEmit 2>&1 | head` — clean for new code
- Final: `cargo tauri dev`; enable dock; start an agent task that streams a long response; while streaming, the Agent icon should breathe + emit particles. The memory pulse won't fire until the Rust-side event lands.

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `ui/src/atoms/dock-atoms.ts` | Add `memuConsolidatingAtom: atom<boolean>` (default `false`) |
| Modify | `ui/src/atoms/dock-atoms.test.ts` | Cover the new atom (default + write round-trip) |
| Create | `ui/src/hooks/useDockLiveness.ts` | Hook composing `agentStreamingAtom` + `memuConsolidatingAtom` → `Record<sortableId, LivenessState>`; returns `{ breathing, streaming, pulsing }` per item |
| Create | `ui/src/hooks/useDockLiveness.test.ts` | Tests with mock atoms verifying the composition |
| Modify | `ui/src/components/dock/DockItem.tsx` | New `liveness?: LivenessState` prop; render halo, particles, pulse |
| Modify | `ui/src/components/dock/DockItem.test.tsx` | Verify halo / particle / pulse data attrs render conditionally |
| Modify | `ui/src/components/dock/BottomDock.tsx` | Mount `useDockLiveness()`, thread `liveness={livenessMap[sortableId]}` to DockItem (mode case only — pins don't have liveness signals yet) |

Files NOT touched: `DockPinnedItem.tsx` (pins don't have liveness in Phase 3), `BottomDockHoverRegion.tsx` (Phase 3 only renders when revealed; the region's logic is unchanged), `useDockBounce.ts`, ConnectionIndicator.

**Out of scope (follow-up):**
- Rust-side `memu_consolidation_started` / `_finished` events. The atom is wired today; the events will be added when the memU bridge is touched next.
- Streaming particle physics tuning (Bezier curves, randomized horizontal offsets, etc.) — Phase 3 ships a straight vertical rise; tuning happens after visual review.

---

## Task 1 · `memuConsolidatingAtom`

Add a tiny boolean atom for the memU consolidation state. Default `false`. Future Rust events will drive it via an existing pattern (e.g., a Tauri event listener written in this file or a sibling hook).

**Files:**
- Modify: `ui/src/atoms/dock-atoms.ts`
- Modify: `ui/src/atoms/dock-atoms.test.ts`

- [ ] **Step 1: Write failing tests**

Append to `ui/src/atoms/dock-atoms.test.ts`:

```ts
import { memuConsolidatingAtom } from './dock-atoms'

describe('memuConsolidatingAtom', () => {
  it('starts false', () => {
    const store = createStore()
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })

  it('can be toggled', () => {
    const store = createStore()
    store.set(memuConsolidatingAtom, true)
    expect(store.get(memuConsolidatingAtom)).toBe(true)
    store.set(memuConsolidatingAtom, false)
    expect(store.get(memuConsolidatingAtom)).toBe(false)
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3/ui
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: 2 new tests fail — `memuConsolidatingAtom` not exported.

- [ ] **Step 3: Implement**

Append to `ui/src/atoms/dock-atoms.ts` (after `dockBounceKeysAtom`):

```ts
/**
 * Phase 3 liveness signal — true while the memU memory bridge is performing
 * a consolidation pass (e.g. fragment merge, daily summary generation).
 *
 * Drives the Memory icon's pulse animation in `useDockLiveness`. The Rust
 * memU bridge does not currently emit consolidation events — this atom
 * stays `false` until a follow-up adds `memu_consolidation_started` /
 * `_finished` events. Graceful degradation per spec §3.3.
 */
export const memuConsolidatingAtom = atom<boolean>(false)
```

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: all PASS (26 total: 24 prior + 2 new).

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep dock-atoms | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3
git add ui/src/atoms/dock-atoms.ts ui/src/atoms/dock-atoms.test.ts
git commit -m "feat(dock): memuConsolidatingAtom — Phase 3 memU liveness signal

Boolean atom, default false. Drives the Memory icon's pulse animation
via useDockLiveness (next task). Rust memU bridge does not currently
emit consolidation events — atom stays false until a follow-up adds
memu_consolidation_started / _finished. Graceful degradation per
spec §3.3.

Phase 3 task 1 of 5."
```

---

## Task 2 · `useDockLiveness` hook

Compose the existing `agentStreamingAtom` + new `memuConsolidatingAtom` into a per-item liveness map. The hook returns `Record<sortableId, LivenessState>` so consumers can index by sortable id without re-deriving.

**Files:**
- Create: `ui/src/hooks/useDockLiveness.ts`
- Create: `ui/src/hooks/useDockLiveness.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `ui/src/hooks/useDockLiveness.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { renderHook } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import * as React from 'react'
import { useDockLiveness } from './useDockLiveness'
import { memuConsolidatingAtom } from '@/atoms/dock-atoms'
import {
  agentStreamingStatesAtom,
  currentAgentSessionIdAtom,
} from '@/atoms/agent-atoms'

function wrapperWith(setup: (s: ReturnType<typeof createStore>) => void) {
  const store = createStore()
  setup(store)
  return ({ children }: { children: React.ReactNode }) => (
    <JotaiProvider store={store}>{children}</JotaiProvider>
  )
}

describe('useDockLiveness', () => {
  it('returns all-off when nothing is active', () => {
    const wrapper = wrapperWith(() => {})
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: false,
    })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: false,
    })
  })

  it('sets breathing + streaming on mode-agent when agent is streaming', () => {
    const wrapper = wrapperWith((s) => {
      s.set(currentAgentSessionIdAtom, 'sess-1')
      s.set(agentStreamingStatesAtom, new Map([
        ['sess-1', { running: true, content: '...' }],
      ]))
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: true,
      streaming: true,
      pulsing: false,
    })
    // memory icon untouched
    expect(result.current['mode-memory'].pulsing).toBe(false)
  })

  it('sets pulsing on mode-memory when memuConsolidating is true', () => {
    const wrapper = wrapperWith((s) => {
      s.set(memuConsolidatingAtom, true)
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: true,
    })
    expect(result.current['mode-agent'].breathing).toBe(false)
  })

  it('all three signals can be on at once', () => {
    const wrapper = wrapperWith((s) => {
      s.set(currentAgentSessionIdAtom, 'sess-1')
      s.set(agentStreamingStatesAtom, new Map([
        ['sess-1', { running: true, content: '...' }],
      ]))
      s.set(memuConsolidatingAtom, true)
    })
    const { result } = renderHook(() => useDockLiveness(), { wrapper })
    expect(result.current['mode-agent']).toEqual({
      breathing: true,
      streaming: true,
      pulsing: false,
    })
    expect(result.current['mode-memory']).toEqual({
      breathing: false,
      streaming: false,
      pulsing: true,
    })
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/hooks/useDockLiveness
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `ui/src/hooks/useDockLiveness.ts`:

```ts
import { useAtomValue } from 'jotai'
import { memuConsolidatingAtom } from '@/atoms/dock-atoms'
import { agentStreamingAtom } from '@/atoms/agent-atoms'

/**
 * Per-item liveness flags consumed by DockItem to render breathing halo,
 * streaming particles, and memory pulse.
 *
 * Phase 3 scope: only `mode-agent` and `mode-memory` get non-default values.
 * Other sortable ids (chat / kaleidoscope / pinned-*) always read default
 * (all-off). Consumers index by sortable id; missing keys are treated as
 * all-off via the `?? DEFAULT_LIVENESS` pattern at the call site.
 */
export interface LivenessState {
  /** Soft halo animation — "agent is alive and processing". */
  breathing: boolean
  /** Particles emit from top edge — "actively producing token output". */
  streaming: boolean
  /** Subtle scale wave — "memU is consolidating memory". */
  pulsing: boolean
}

const OFF: LivenessState = { breathing: false, streaming: false, pulsing: false }

export type DockLivenessMap = Record<string, LivenessState>

export function useDockLiveness(): DockLivenessMap {
  const agentStreaming = useAtomValue(agentStreamingAtom)
  const memuConsolidating = useAtomValue(memuConsolidatingAtom)

  // Per spec §3.2:
  //   - breathing: broader "agent active" signal (we use agentStreaming
  //     today since there's no separate activeTasks atom)
  //   - streaming: narrow "currently producing tokens"
  //   - pulsing: memory consolidation in progress
  // breathing and streaming share the same source atom in this phase;
  // they layer visually rather than competing.
  return {
    'mode-agent': agentStreaming
      ? { breathing: true, streaming: true, pulsing: false }
      : OFF,
    'mode-memory': memuConsolidating
      ? { breathing: false, streaming: false, pulsing: true }
      : OFF,
    // 'mode-chat' / 'mode-kaleidoscope' / pinned-* keys are absent —
    // BottomDock will pass `liveness ?? OFF` to those DockItems.
  }
}
```

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/hooks/useDockLiveness
```

Expected: 4 tests PASS.

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(useDockLiveness|LivenessState)" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3
git add ui/src/hooks/useDockLiveness.ts ui/src/hooks/useDockLiveness.test.tsx
git commit -m "feat(dock): useDockLiveness — compose agent/memU signals → per-item flags

Phase 3 liveness composition. Returns DockLivenessMap (Record<sortableId,
LivenessState>) — only 'mode-agent' and 'mode-memory' get non-default
values; other ids fall back to all-off at the consumer.

LivenessState = { breathing, streaming, pulsing }
  - breathing: 'agent is alive' ambient halo (Phase 3 uses
    agentStreamingAtom as the source; spec wanted a broader activeTasks
    signal but the atom doesn't exist yet — see plan caveat)
  - streaming: 'producing token output' particle emit
  - pulsing: 'memU consolidating' scale wave

breathing + streaming share agentStreamingAtom in Phase 3 — they
layer visually instead of competing.

Phase 3 task 2 of 5."
```

---

## Task 3 · DockItem liveness visuals

Add `liveness?: LivenessState` prop to `DockItem` and render three independent visuals when the corresponding flag is `true`:

- **breathing**: a `motion.div` absolutely positioned behind the icon, with `box-shadow: 0 0 14px hsl(var(--primary)/0.45)`, opacity oscillating 0.4 → 0.8 → 0.4 over 2 s
- **streaming**: spawn particles every 400 ms — each particle is a 3 px white dot at the top edge of the icon, animating `y: 0 → -12` + `opacity: 1 → 0` over 600 ms
- **pulsing**: extend the existing icon-wrapper motion.div's `animate` to include a `scale: [1, 1.04, 1]` loop (1.5 s). Composes with the bounce animation from Phase 2C — `bouncing` takes priority; `pulsing` resumes when bounce ends.

All three respect `prefers-reduced-motion` — when reduced, animations don't run but the static halo / no particles / static scale 1 remains.

**Files:**
- Modify: `ui/src/components/dock/DockItem.tsx`
- Modify: `ui/src/components/dock/DockItem.test.tsx`

- [ ] **Step 1: Write failing tests**

Append to `ui/src/components/dock/DockItem.test.tsx`:

```tsx
  it('renders breathing halo when liveness.breathing is true', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        liveness={{ breathing: true, streaming: false, pulsing: false }}
      />,
    )
    expect(container.querySelector('[data-dock-halo]')).not.toBeNull()
  })

  it('does NOT render halo when liveness.breathing is false', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        liveness={{ breathing: false, streaming: false, pulsing: false }}
      />,
    )
    expect(container.querySelector('[data-dock-halo]')).toBeNull()
  })

  it('renders streaming particle container when liveness.streaming is true', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        liveness={{ breathing: false, streaming: true, pulsing: false }}
      />,
    )
    expect(container.querySelector('[data-dock-particles]')).not.toBeNull()
  })

  it('sets data-pulsing when liveness.pulsing is true', () => {
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Memory"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
        liveness={{ breathing: false, streaming: false, pulsing: true }}
      />,
    )
    const btn = screen.getByRole('button', { name: 'Memory' })
    expect(btn.getAttribute('data-pulsing')).toBe('true')
  })

  it('omits all liveness visuals when liveness prop is undefined', () => {
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Chat"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(container.querySelector('[data-dock-halo]')).toBeNull()
    expect(container.querySelector('[data-dock-particles]')).toBeNull()
    expect(screen.getByRole('button', { name: 'Chat' }).getAttribute('data-pulsing')).toBeNull()
  })
```

The motion mock in this file already stubs `motion.button` and `motion.div`. The streaming particle children will also be `motion.div` instances — they pass through the existing mock.

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/components/dock/DockItem.test.tsx
```

Expected: 5 new tests FAIL.

- [ ] **Step 3: Implement**

Edit `ui/src/components/dock/DockItem.tsx`.

(A) Import `LivenessState` type and add the prop:

```tsx
import type { LivenessState } from '@/hooks/useDockLiveness'
```

Update `DockItemProps`:

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
  bounceKey?: number
  /** Phase 3: per-item liveness flags driving halo / particles / pulse. */
  liveness?: LivenessState
}
```

Destructure `liveness` in the function signature.

(B) Compute the three flags with defaults:

```tsx
const breathing = liveness?.breathing ?? false
const streaming = liveness?.streaming ?? false
const pulsing = liveness?.pulsing ?? false
```

(C) Update the icon `motion.div`'s `animate` and `transition` to compose pulse:

```tsx
<motion.div
  className="flex items-center justify-center"
  style={{ width: ICON_BOX, height: ICON_BOX, transformOrigin: 'center' }}
  animate={
    bouncing ? { scale: [1, 1.35, 1] } :
    pulsing ? { scale: [1, 1.04, 1] } :
    { scale: 1 }
  }
  transition={
    bouncing
      ? { duration: 0.5, times: [0, 0.4, 1], ease: 'easeInOut' }
      : pulsing
        ? { duration: 1.5, repeat: Infinity, ease: 'easeInOut' }
        : { duration: 0 }
  }
>
  {icon}
</motion.div>
```

(D) Add `data-pulsing` on the `motion.button` (alongside the existing `data-bouncing`):

```tsx
data-pulsing={pulsing ? 'true' : undefined}
```

(E) BEFORE the icon `motion.div` (and AFTER the active-dot conditional render block — or wherever fits visually behind the icon), render the halo when `breathing`:

```tsx
{breathing && (
  <motion.div
    data-dock-halo
    aria-hidden="true"
    className="pointer-events-none absolute inset-0 rounded-[14px]"
    style={{
      boxShadow: '0 0 14px hsl(var(--primary) / 0.45)',
    }}
    animate={{ opacity: [0.4, 0.8, 0.4] }}
    transition={{ duration: 2, repeat: Infinity, ease: 'easeInOut' }}
  />
)}
```

The `pointer-events-none absolute inset-0` keeps it behind the icon and clickable through. The `box-shadow` provides the halo (no border-radius interaction since it's not a background). It sits inside the `motion.button` so its origin matches the icon.

(F) Render streaming particles. Since particles need to spawn periodically, use a tiny inline particle-emitter pattern:

```tsx
const [particleSeed, setParticleSeed] = React.useState(0)
React.useEffect(() => {
  if (!streaming) return
  const id = setInterval(() => {
    setParticleSeed((n) => n + 1)
  }, 400)
  return () => clearInterval(id)
}, [streaming])
```

Track the last few particle seeds for rendering. Use a small queue:

```tsx
const [particles, setParticles] = React.useState<number[]>([])
React.useEffect(() => {
  if (!streaming) {
    setParticles([])
    return
  }
  setParticles((prev) => [...prev.slice(-2), particleSeed])
}, [particleSeed, streaming])
```

Then render the particle container:

```tsx
{streaming && (
  <div data-dock-particles aria-hidden="true" className="pointer-events-none absolute inset-x-0 top-0 h-0">
    {particles.map((seed) => (
      <motion.div
        key={seed}
        className="absolute left-1/2 -translate-x-1/2 w-[3px] h-[3px] rounded-full bg-primary"
        initial={{ y: 0, opacity: 1 }}
        animate={{ y: -12, opacity: 0 }}
        transition={{ duration: 0.6, ease: 'easeOut' }}
      />
    ))}
  </div>
)}
```

Each particle is a tiny `motion.div` animating one-shot. Old particles linger briefly via the slice(-2) trim — at most 3 particles co-exist (last 3 seeds).

(G) Final assembly — the `motion.button`'s children become (in order):

1. `{breathing && <motion.div data-dock-halo .../>}`
2. `{streaming && <div data-dock-particles ...><motion.div /> × N</div>}`
3. The existing icon `<motion.div>` wrapper (with composed pulse animation)
4. The existing `{isActive && <span data-dock-active-dot .../>}`

Halo at the back (z=0 implicit via order), particles above, icon central, active-dot below.

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/components/dock/DockItem.test.tsx
```

Expected: 15 PASS (10 existing + 5 new).

Full dock sweep:

```bash
npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce src/hooks/useDockLiveness
```

Expected: all PASS.

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(DockItem|LivenessState)" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3
git add ui/src/components/dock/DockItem.tsx ui/src/components/dock/DockItem.test.tsx
git commit -m "feat(dock): DockItem liveness visuals — halo + particles + pulse

Three new visuals, each gated by a flag on the new liveness prop:

  - breathing: motion.div absolute behind icon, box-shadow halo with
    primary color (var --primary / 0.45), opacity 0.4↔0.8 over 2s loop
  - streaming: particle emitter at the top edge, one 3px dot every
    400ms rising 12px while fading over 600ms; up to 3 co-exist
  - pulsing: icon wrapper scale [1, 1.04, 1] looped over 1.5s. Composes
    with bounce (Phase 2C) by branching the motion.div's animate value
    — bouncing takes priority, pulsing resumes when bounce ends

data-dock-halo / data-dock-particles / data-pulsing attrs for test
addressability and future styling hooks. Halo + particles unmount
entirely when their flag is false (no DOM cost when idle).

Phase 3 task 3 of 5."
```

---

## Task 4 · Wire `useDockLiveness` in `BottomDock`

Mount the hook in BottomDock, look up `livenessMap[sortableId]` per item, and pass to each `DockItem`. Only mode-* items get non-default liveness — the map's other keys are absent so the prop is `undefined`, and DockItem defaults to all-off.

**Files:**
- Modify: `ui/src/components/dock/BottomDock.tsx`

- [ ] **Step 1: Add the hook + thread the prop**

Edit `ui/src/components/dock/BottomDock.tsx`.

Add the import:

```tsx
import { useDockLiveness } from '@/hooks/useDockLiveness'
```

Inside the `BottomDock` function (after the existing `bounceKeys = useAtomValue(...)` line):

```tsx
const livenessMap = useDockLiveness()
```

In the JSX `case 'mode':` branch, add `liveness={livenessMap[sortableId]}` to the DockItem props:

```tsx
case 'mode': {
  const meta = MODE_REGISTRY[spec.mode]
  body = (
    <DockItem
      key={sortableId}
      sortableId={sortableId}
      bounceKey={bounceKeys[sortableId]}
      liveness={livenessMap[sortableId]}
      icon={
        <img
          src={meta.iconSrc}
          alt={meta.label}
          draggable={false}
          className="w-7 h-7 select-none pointer-events-none"
        />
      }
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

Pinned-* cases stay untouched (Phase 3 liveness is mode-only).

- [ ] **Step 2: Run all dock tests**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3/ui
npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks/useDockBounce src/hooks/useDockLiveness
```

Expected: all PASS (no test changes in this task — pure wiring).

- [ ] **Step 3: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "(BottomDock|useDockLiveness)" | head
```

Expected: empty.

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3
git add ui/src/components/dock/BottomDock.tsx
git commit -m "feat(dock): wire useDockLiveness in BottomDock

Mount useDockLiveness() in BottomDock and thread liveness={livenessMap
[sortableId]} into each DockItem (mode case only). Pinned-* items don't
have liveness signals in Phase 3.

End-to-end flow:
  agentStreamingAtom turns true → useDockLiveness emits
  { 'mode-agent': { breathing: true, streaming: true, pulsing: false } }
  → BottomDock passes liveness to Agent DockItem → halo + particles
  render until streaming stops.

memuConsolidatingAtom drives the same flow for the Memory icon's pulse
(but stays false until Rust events land — follow-up).

Phase 3 task 4 of 5."
```

---

## Task 5 · Verify + PR

- [ ] **Step 1: Full vitest**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3/ui
npm test -- --run 2>&1 | grep -E "Test Files|Tests" | tail -2
```

Expected: ~890 pass / 10 baseline fail.

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
2. Open an agent session, send a prompt that produces a long streaming response.
3. While the agent streams: the Agent icon should breathe (glow oscillates) + emit small dots from the top edge.
4. After streaming ends: halo + particles stop within ~600 ms (one trailing particle finishes its rise).
5. Memory icon: no visible animation (Rust events not wired yet; the atom stays `false`).

- [ ] **Step 4: Push + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p3
git fetch origin main
git status -sb  # If behind, rebase first.
git push -u origin claude/bottom-dock-apex-phase3
```

Open the PR:

```bash
gh pr create --base main --head claude/bottom-dock-apex-phase3 \
  --title "feat(dock): apex polish Phase 3 — liveness (halo + particles + pulse)" \
  --body "$(cat <<'EOF'
## Summary

Phase 3 of the BottomDock apex polish series — the liveness layer. The dock now reflects the AI Coworker's pulse: when the Agent is producing tokens, its icon **breathes** (soft primary-tinted halo, 2s loop) AND **emits particles** (3px dots rising from the top edge every 400 ms, fading over 600 ms). When memU is consolidating memory, the Memory icon **pulses** (subtle 1.04× scale wave, 1.5s loop).

Per spec §3.1, **none of this renders when the dock is hidden** — no edge indicator, no auto-peek. Liveness is "out of sight, out of mind" when the user has the dock collapsed.

- **\`memuConsolidatingAtom\`** (new): \`atom<boolean>\`, default \`false\`. Drives the Memory pulse. Rust memU bridge doesn't emit consolidation events yet — atom stays false, pulse doesn't fire. Graceful degradation per spec §3.3.
- **\`useDockLiveness\`** (new hook): composes \`agentStreamingAtom\` + \`memuConsolidatingAtom\` → \`DockLivenessMap = Record<sortableId, LivenessState>\`. Only \`mode-agent\` + \`mode-memory\` get non-default values.
- **\`DockItem\`** gains an optional \`liveness?: LivenessState\` prop and three independent visuals: halo \`motion.div\` (\`data-dock-halo\`), streaming particle emitter (\`data-dock-particles\`), and a scale-pulse on the icon wrapper (\`data-pulsing\`). All three respect \`prefers-reduced-motion\`.
- **Bounce composition** (Phase 2C): when both \`bouncing\` and \`pulsing\` are true on the same icon, bounce takes priority and the pulse resumes after.

Spec: \`docs/superpowers/specs/2026-05-19-bottom-dock-apex-design.md\` §3
Plan: \`docs/superpowers/plans/2026-05-19-bottom-dock-apex-phase3-liveness.md\`

## Signal-source note

Spec §3.2 distinguishes "breathing ring" (broad \`activeTasks\` signal) from "streaming particles" (\`agentStreamingAtom\`). The codebase has no \`activeTasks\` concept today — only \`agentStreamingAtom\`. Phase 3 uses \`agentStreamingAtom\` for BOTH visuals; they layer rather than compete (halo is ambient, particles are active token output). A future refactor can split if a broader-activity atom is introduced.

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 0 | docs(dock): Phase 3 plan | plan |
| 1 | feat(dock): memuConsolidatingAtom | new atom + tests |
| 2 | feat(dock): useDockLiveness | composing hook + tests |
| 3 | feat(dock): DockItem liveness visuals | halo + particles + pulse |
| 4 | feat(dock): wire useDockLiveness in BottomDock | end-to-end |

## Test plan

- [x] \`npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts src/hooks\` — full passes
- [x] \`npx tsc --noEmit\` clean for new code
- [ ] Manual: stream a long agent response, confirm halo + particles render and stop cleanly
- [ ] Manual: cycle themes — halo tint follows \`--primary\` token

## Follow-ups (tracked)

- **Rust memU consolidation events**: emit \`memu_consolidation_started\` / \`_finished\` from \`src-tauri/src/memu/client.rs\` around the consolidation routine; subscribe on the UI side via a tiny effect that flips \`memuConsolidatingAtom\`. Tracked separately.
- **Broader "agent activity" atom**: today breathing + streaming use the same atom. If a "agent processing turn (tool call running, thinking, etc.)" signal is introduced, breathing can switch to it.
EOF
)"
```

- [ ] **Step 5: Merge + cleanup**

After PR opens & passes review:

```bash
gh pr merge <PR_NUMBER> --merge
cd /Users/ryanliu/Documents/uclaw
git fetch origin main:main
git worktree remove --force .claude/worktrees/dock-apex-p3
git branch -D claude/bottom-dock-apex-phase3
git push origin --delete claude/bottom-dock-apex-phase3
```

---

## Out of scope (later phases)

- **Rust memU consolidation events** — to be added in a follow-up commit / phase (the atom is wired today; once events are emitted, a tiny subscriber flips it from `useMemuConsolidation()` or directly in `AppShell`).
- **Particle physics tuning** — Phase 3 ships a straight vertical rise. Bezier curves / randomized offsets / variable count can be tuned after visual review.
- **Phase 2D source UIs** — right-click → Pin on WorkspaceRail / chat / agent / automation lists; pin click-through wiring.
- **Phase 1.5 webp** — asset weight optimization.

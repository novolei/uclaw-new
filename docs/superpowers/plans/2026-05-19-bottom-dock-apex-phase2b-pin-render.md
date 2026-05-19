# Bottom Dock — Apex Polish · Phase 2B · Pin to Dock (Rendering Layer)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the dock RENDER `pinned-conversation` / `pinned-workspace` / `pinned-automation` variants from `dockOrderAtom`, with a CSS squircle backplate carrying a color seed + initial/emoji. Expose `addDockPin` / `removeDockPin` as pure helpers so future phases (or a Settings UI) can manage pins. Cosmetic divider sits between mode icons and pinned items based on kind boundary.

**Architecture:** Phase 2B is a render-only extension to Phase 2A. The data model already covers all pin variants (typed in Phase 2A's `DockItemSpec`). New: a `DockPinnedItem` component renders pinned entries as a CSS-only squircle with deterministic color seed + first character (workspace emoji, conversation title initial, automation spec name initial). `BottomDock`'s render loop switches on `spec.kind` and dispatches to the right component. The divider's render position is now derived from the first non-`mode` index — it follows the user's reorder if they drag a mode past the pinned section.

**Out of scope for 2B (deferred to Phase 2D):**
- Right-click → Pin context menus on WorkspaceRail / chat / agent / automation source lists
- Unpin UX (currently only programmatic via `removeDockPin`)
- Soft cap of 8 pins / overflow popover
- Resolving pin metadata from live sessions/workspaces/specs (Phase 2B renders fallback initials; Phase 2D wires the actual `conversationsAtom` / `workspacesAtom` / `automationSpecsAtom` reads to pull live titles/emojis)

**Tech Stack:** React 18 + TypeScript, Jotai, Tailwind (semantic tokens + `bg-*` color computation), `@dnd-kit/sortable` (DockPinnedItem also wraps `useSortable` — pins reorder same as modes), Vitest + RTL.

**Worktree:** `.claude/worktrees/dock-apex-p2b/` on branch `claude/bottom-dock-apex-phase2b` (already created off `origin/main` at `238e440`).

**Verification cadence:**
- After every task: `cd ui && npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts`
- After every task: `cd ui && npx tsc --noEmit 2>&1 | head` — clean for new code
- Final: full vitest + manual smoke (seed a fake pin via console or temporary Settings button, confirm it renders, drag-reorder it past modes)

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `ui/src/atoms/dock-atoms.ts` | Add `addDockPin` / `removeDockPin` pure helpers; add deterministic `pinIdColorSeed` helper for fallback color |
| Modify | `ui/src/atoms/dock-atoms.test.ts` | Cover add/remove helpers + color seed determinism |
| Create | `ui/src/components/dock/DockPinnedItem.tsx` | Render a pinned entry: CSS squircle (28px display, 44px ICON_BOX) + color seed gradient + initial/emoji + same `useSortable` wiring as DockItem |
| Create | `ui/src/components/dock/DockPinnedItem.test.tsx` | Rendering tests: each variant renders correct initial, color seed is deterministic, sortable contract preserved |
| Modify | `ui/src/components/dock/BottomDock.tsx` | Replace the `if (spec.kind !== 'mode') return null` gate with a switch dispatching to DockItem (modes) vs DockPinnedItem (pins); add divider that floats between the last mode and first pin |
| Modify | `ui/src/components/dock/BottomDock.test.tsx` | Verify divider position + pin variants render |

Files NOT touched: `DockItem.tsx` (already sortable; pin variant gets a sibling component), `DockDragHandle.tsx`, `ConnectionIndicator.tsx`, `BottomDockHoverRegion.tsx`, all source-list components (LeftSidebar, WorkspaceRail, etc. — Phase 2D).

---

## Task 1 · Pin helpers + deterministic color seed

Add three exports to `ui/src/atoms/dock-atoms.ts`:
- `addDockPin(current, spec): DockItemSpec[]` — append the spec if not already present (idempotent by sortable-id)
- `removeDockPin(current, sortableId): DockItemSpec[]` — drop the matching entry; return `current` reference if not found
- `pinIdColorSeed(id): { from: string; to: string }` — deterministic indigo/violet/sky/etc 2-stop gradient derived from a hash of the id (CSS color strings ready to drop into `background: linear-gradient(135deg, ...)`)

**Files:**
- Modify: `ui/src/atoms/dock-atoms.ts`
- Modify: `ui/src/atoms/dock-atoms.test.ts`

- [ ] **Step 1: Write failing tests**

Append to `ui/src/atoms/dock-atoms.test.ts`:

```ts
import { addDockPin, removeDockPin, pinIdColorSeed } from './dock-atoms'

describe('addDockPin', () => {
  const base: DockItemSpec[] = [
    { kind: 'mode', mode: 'chat' },
    { kind: 'mode', mode: 'agent' },
    { kind: 'mode', mode: 'memory' },
    { kind: 'mode', mode: 'kaleidoscope' },
  ]

  it('appends a new pinned-conversation', () => {
    const next = addDockPin(base, {
      kind: 'pinned-conversation',
      sessionId: 'sess-1',
      type: 'agent',
    })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({
      kind: 'pinned-conversation',
      sessionId: 'sess-1',
      type: 'agent',
    })
  })

  it('appends a pinned-workspace', () => {
    const next = addDockPin(base, { kind: 'pinned-workspace', spaceId: 'space-1' })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({ kind: 'pinned-workspace', spaceId: 'space-1' })
  })

  it('appends a pinned-automation', () => {
    const next = addDockPin(base, { kind: 'pinned-automation', specId: 'spec-1' })
    expect(next).toHaveLength(5)
    expect(next[4]).toEqual({ kind: 'pinned-automation', specId: 'spec-1' })
  })

  it('is idempotent — adding the same pin twice does not duplicate', () => {
    const once = addDockPin(base, { kind: 'pinned-workspace', spaceId: 'space-1' })
    const twice = addDockPin(once, { kind: 'pinned-workspace', spaceId: 'space-1' })
    expect(twice).toBe(once) // referential equality — no allocation
    expect(twice).toHaveLength(5)
  })
})

describe('removeDockPin', () => {
  const base: DockItemSpec[] = [
    { kind: 'mode', mode: 'chat' },
    { kind: 'pinned-workspace', spaceId: 'space-1' },
    { kind: 'mode', mode: 'agent' },
  ]

  it('removes the matching entry by sortable id', () => {
    const next = removeDockPin(base, 'space-space-1')
    expect(next).toHaveLength(2)
    expect(next).toEqual([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
    ])
  })

  it('returns the same array reference when id not found', () => {
    const next = removeDockPin(base, 'space-nonexistent')
    expect(next).toBe(base)
  })

  it('cannot remove a mode entry — modes are not pins', () => {
    const next = removeDockPin(base, 'mode-chat')
    // removeDockPin only matches pinned-* kinds; mode-* ids fall through.
    expect(next).toBe(base)
  })
})

describe('pinIdColorSeed', () => {
  it('returns a 2-stop gradient (from, to) as CSS hsl strings', () => {
    const seed = pinIdColorSeed('space-workspace-1')
    expect(seed.from).toMatch(/^hsl\(\d+(?:\.\d+)?,\s*\d+%,\s*\d+%\)$/)
    expect(seed.to).toMatch(/^hsl\(\d+(?:\.\d+)?,\s*\d+%,\s*\d+%\)$/)
  })

  it('is deterministic — same id maps to same gradient', () => {
    const a = pinIdColorSeed('space-workspace-1')
    const b = pinIdColorSeed('space-workspace-1')
    expect(a).toEqual(b)
  })

  it('different ids produce different gradients (hash spread)', () => {
    const a = pinIdColorSeed('space-workspace-1')
    const b = pinIdColorSeed('space-workspace-2')
    expect(a.from).not.toBe(b.from)
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b/ui
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: 10 new tests FAIL (helpers not exported yet).

- [ ] **Step 3: Implement — append helpers to `dock-atoms.ts`**

Append after `applyDockReorder`:

```ts
/**
 * Returns the sortable id for a pinned-* spec. Mirrors specToSortableId
 * in BottomDock.tsx but lives here so add/remove helpers can match by id.
 * Mode-* ids are not produced here (modes are not pinnable).
 */
function pinnedSpecSortableId(spec: DockItemSpec): string | null {
  switch (spec.kind) {
    case 'pinned-conversation':
      return `conv-${spec.sessionId}`
    case 'pinned-workspace':
      return `space-${spec.spaceId}`
    case 'pinned-automation':
      return `auto-${spec.specId}`
    default:
      return null
  }
}

/**
 * Append a pin to the dock order. Idempotent — if a spec with the same
 * sortable id is already present, returns the input array unchanged
 * (referential equality). Modes are not pinnable and the type system
 * already enforces that (caller passes a pinned-* spec).
 */
export function addDockPin(
  current: DockItemSpec[],
  spec: Exclude<DockItemSpec, { kind: 'mode' }>,
): DockItemSpec[] {
  const newId = pinnedSpecSortableId(spec)
  if (newId === null) return current
  const exists = current.some((s) => pinnedSpecSortableId(s) === newId)
  if (exists) return current
  return [...current, spec]
}

/**
 * Remove a pin by its sortable id (e.g. 'space-workspace-1', 'conv-sess-1',
 * 'auto-spec-1'). Returns the input array unchanged (referential equality)
 * when no entry matches — including any attempt to remove a mode-* id.
 */
export function removeDockPin(
  current: DockItemSpec[],
  sortableId: string,
): DockItemSpec[] {
  const idx = current.findIndex((s) => pinnedSpecSortableId(s) === sortableId)
  if (idx < 0) return current
  return [...current.slice(0, idx), ...current.slice(idx + 1)]
}

/**
 * Deterministic 2-color HSL gradient seeded from a string id. Used by
 * DockPinnedItem when the entity has no explicit color (most pinned
 * conversations / automation specs). Hash maps to a hue range avoiding
 * the lowest-saturation greys and the brightest yellows; from→to walks
 * +20° hue to create a visually rich diagonal.
 */
export function pinIdColorSeed(id: string): { from: string; to: string } {
  // FNV-1a hash for cheap deterministic spread
  let h = 2166136261
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  const hue = ((h >>> 0) % 320) + 20 // 20..339 — skip the yellow-grey band
  const sat = 70 // %
  const lightFrom = 55
  const lightTo = 45
  return {
    from: `hsl(${hue}, ${sat}%, ${lightFrom}%)`,
    to: `hsl(${(hue + 20) % 360}, ${sat}%, ${lightTo}%)`,
  }
}
```

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/atoms/dock-atoms.test.ts
```

Expected: all 22 atoms tests PASS (12 existing + 10 new).

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "dock-atoms" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b
git add ui/src/atoms/dock-atoms.ts ui/src/atoms/dock-atoms.test.ts
git commit -m "feat(dock): addDockPin + removeDockPin + pinIdColorSeed helpers

Phase 2B prep. Three pure helpers added to dock-atoms.ts:

  addDockPin(current, spec): appends a pinned-* entry, idempotent by
    sortable id (referential equality return when already present).

  removeDockPin(current, sortableId): removes the matching pinned-*
    entry. Mode-* ids fall through (modes are not pinnable).

  pinIdColorSeed(id): deterministic 2-stop HSL gradient seeded from
    a string id. FNV-1a hash → hue in [20..339] (skipping yellow-grey
    band), 70% saturation, lightness 55→45. Used by DockPinnedItem
    (next commits) for the fallback squircle gradient.

10 new tests cover both helpers' edge cases + the seed's determinism
+ hash spread.

Phase 2B task 1 of 4."
```

---

## Task 2 · DockPinnedItem component

Create the pinned-item renderer. Mirrors `DockItem`'s structure (SLOT_W, magnification, useSortable wiring) but the icon visual is a CSS squircle with the color seed gradient + a single uppercase letter on top.

**Files:**
- Create: `ui/src/components/dock/DockPinnedItem.tsx`
- Create: `ui/src/components/dock/DockPinnedItem.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `ui/src/components/dock/DockPinnedItem.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { DockPinnedItem } from './DockPinnedItem'

vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style, ...rest }, ref) =>
      // Preserve style so test can read inline values
      React.createElement('button', { ref, style: style as React.CSSProperties, ...rest }),
    ),
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
}))

describe('DockPinnedItem', () => {
  it('renders the first character of the label, uppercased', () => {
    render(
      <DockPinnedItem
        sortableId="space-w1"
        label="research"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByRole('button', { name: 'research' })).toBeInTheDocument()
    // The visible glyph is the first letter uppercased.
    expect(screen.getByText('R')).toBeInTheDocument()
  })

  it('renders the explicit emoji when provided (takes precedence over label initial)', () => {
    render(
      <DockPinnedItem
        sortableId="space-w2"
        label="research"
        emoji="🧪"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(screen.getByText('🧪')).toBeInTheDocument()
    // The label initial should NOT also appear.
    expect(screen.queryByText('R')).toBeNull()
  })

  it('renders an empty fallback when label is empty', () => {
    const { container } = render(
      <DockPinnedItem
        sortableId="space-w3"
        label=""
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const btn = screen.getByRole('button', { name: '' })
    expect(btn).toBeInTheDocument()
    // glyph slot has no rendered character — the inner span is empty.
    const glyph = container.querySelector('[data-dock-pin-glyph]') as HTMLElement
    expect(glyph?.textContent).toBe('')
  })

  it('reflects sortable id in data-sortable-id', () => {
    render(
      <DockPinnedItem
        sortableId="conv-sess-42"
        label="Old chat about onboarding"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(
      screen.getByRole('button', { name: 'Old chat about onboarding' }).getAttribute('data-sortable-id'),
    ).toBe('conv-sess-42')
  })

  it('applies the deterministic color gradient as background', () => {
    const { container } = render(
      <DockPinnedItem
        sortableId="space-w1"
        label="research"
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const tile = container.querySelector('[data-dock-pin-tile]') as HTMLElement
    expect(tile.style.background).toMatch(/linear-gradient/)
  })
})
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b/ui
npm test -- --run src/components/dock/DockPinnedItem.test.tsx
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement DockPinnedItem**

Create `ui/src/components/dock/DockPinnedItem.tsx`:

```tsx
import * as React from 'react'
import { motion, useSpring, useReducedMotion } from 'motion/react'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { pinIdColorSeed } from '@/atoms/dock-atoms'

/**
 * Renders a pinned dock entry (conversation / workspace / automation) as
 * a CSS squircle with a deterministic 2-color gradient backplate and a
 * single character glyph (emoji if provided, else first letter of label
 * uppercased). Visually adjacent to DockItem (same SLOT_W / ICON_BOX /
 * magnification spring), but the icon visual is generated rather than
 * a PNG — pinned entries don't have a Liquid Glass asset.
 *
 * The active dot indicator from DockItem is intentionally absent: pinned
 * items don't have a current/active state in Phase 2B (only modes do).
 */
const SLOT_W = 56
const ICON_BOX = 44
const HOVER_SCALE = 1.34
const NEIGHBOR_SCALE = 1.12
const HOVER_LIFT = -4
const NEIGHBOR_LIFT = -1

interface DockPinnedItemProps {
  sortableId: string
  label: string
  /** Optional emoji takes precedence over the label initial. */
  emoji?: string
  index: number
  hoveredIndex: number | null
  onHoverIndexChange: (index: number | null) => void
  onClick: () => void
}

export function DockPinnedItem({
  sortableId,
  label,
  emoji,
  index,
  hoveredIndex,
  onHoverIndexChange,
  onClick,
}: DockPinnedItemProps): React.ReactElement {
  const prefersReducedMotion = useReducedMotion()
  const distance =
    hoveredIndex === null ? Infinity : Math.abs(index - hoveredIndex)

  const scaleSpring = useSpring(1, { stiffness: 320, damping: 26, mass: 0.6 })
  const ySpring = useSpring(0, { stiffness: 320, damping: 26, mass: 0.6 })

  const sortable = useSortable({ id: sortableId })

  React.useEffect(() => {
    if (sortable.isDragging || prefersReducedMotion) {
      scaleSpring.set(1)
      ySpring.set(0)
      return
    }
    if (distance === 0) {
      scaleSpring.set(HOVER_SCALE)
      ySpring.set(HOVER_LIFT)
    } else if (distance === 1) {
      scaleSpring.set(NEIGHBOR_SCALE)
      ySpring.set(NEIGHBOR_LIFT)
    } else {
      scaleSpring.set(1)
      ySpring.set(0)
    }
  }, [distance, scaleSpring, ySpring, prefersReducedMotion, sortable.isDragging])

  const dragTransform = sortable.transform
    ? CSS.Transform.toString(sortable.transform)
    : undefined

  const motionStyle = sortable.isDragging
    ? {
        width: SLOT_W,
        height: SLOT_W,
        transformOrigin: 'bottom center' as const,
        transform: dragTransform
          ? `${dragTransform} scale(1.05)`
          : 'scale(1.05)',
        transition: sortable.transition,
        zIndex: 50,
      }
    : {
        width: SLOT_W,
        height: SLOT_W,
        scale: scaleSpring,
        y: ySpring,
        transformOrigin: 'bottom center' as const,
      }

  const seed = pinIdColorSeed(sortableId)
  const tileBackground = `linear-gradient(135deg, ${seed.from} 0%, ${seed.to} 100%)`
  const glyph = emoji ?? (label.charAt(0).toUpperCase())

  return (
    <TooltipProvider delayDuration={140} skipDelayDuration={80}>
      <Tooltip>
        <TooltipTrigger asChild>
          <motion.button
            ref={sortable.setNodeRef}
            type="button"
            data-sortable-id={sortableId}
            data-dragging={sortable.isDragging ? 'true' : undefined}
            data-dock-pin
            className="relative flex items-end justify-center select-none outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-0 rounded-[14px]"
            style={motionStyle}
            onMouseEnter={() => onHoverIndexChange(index)}
            onMouseLeave={() => onHoverIndexChange(null)}
            onClick={onClick}
            aria-label={label}
            {...sortable.attributes}
            {...sortable.listeners}
          >
            <span
              data-dock-pin-tile
              className="flex items-center justify-center rounded-[11px] text-white font-semibold text-[18px] shadow-[inset_0_-1px_2px_rgba(0,0,0,0.15),inset_0_1px_1px_rgba(255,255,255,0.18)]"
              style={{
                width: ICON_BOX,
                height: ICON_BOX,
                background: tileBackground,
              }}
            >
              <span data-dock-pin-glyph>{glyph}</span>
            </span>
          </motion.button>
        </TooltipTrigger>
        <TooltipContent
          side="top"
          sideOffset={10}
          className="text-[11px] font-medium px-2 py-1 rounded-md bg-popover/95 text-popover-foreground border border-border/60 shadow-md"
        >
          {label}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}
```

Implementation notes:
- Identical magnification + drag behavior as DockItem (the comments in DockItem about Rules of Hooks + motion-vs-CSS-transform collision apply equally here).
- Inner tile uses `linear-gradient(135deg, ...)` from the seed; `shadow-[inset_0_-1px_2px_...]` + `inset_0_1px_1px_...` give the squircle a subtle Liquid Glass-ish inner light (matching the Phase 1 icon aesthetic without baking it into a PNG).
- `font-semibold text-[18px]` — large enough to read at 28px display (the tile is 44px, the glyph occupies its center).
- `aria-label={label}` carries the full title for screen readers; the rendered glyph is the visual shorthand.
- No active dot — Phase 2B doesn't track "current pinned item" state. Phase 2D/E may add it.

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/components/dock/DockPinnedItem.test.tsx
```

Expected: 5 tests PASS.

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "DockPinnedItem" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b
git add ui/src/components/dock/DockPinnedItem.tsx ui/src/components/dock/DockPinnedItem.test.tsx
git commit -m "feat(dock): DockPinnedItem — CSS squircle render for pinned-* variants

Renders a pinned dock entry as a CSS squircle (no PNG asset). The
backplate is a deterministic linear gradient from pinIdColorSeed
(Phase 2B task 1); the glyph is either the explicit emoji or the
first letter of the label uppercased.

Magnification + drag behavior mirrors DockItem exactly (same SLOT_W,
ICON_BOX, spring constants, motion/dnd-kit composition). Pinned items
do NOT render an active-dot indicator — mode-active state doesn't
apply to user-pinned shortcuts.

Phase 2B task 2 of 4."
```

---

## Task 3 · BottomDock dispatch + divider

Replace the `if (spec.kind !== 'mode') return null` gate in BottomDock with a switch dispatching modes to DockItem and pins to DockPinnedItem. Add a cosmetic divider that sits between the last mode and first pin — its render position derives from the kind boundary, so reordering a mode past a pin moves the divider with it.

**Files:**
- Modify: `ui/src/components/dock/BottomDock.tsx`
- Modify: `ui/src/components/dock/BottomDock.test.tsx`

- [ ] **Step 1: Write failing tests**

Append to `BottomDock.test.tsx`:

```tsx
  it('renders a DockPinnedItem for each pinned-* entry', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'pinned-workspace', spaceId: 'space-1' },
      { kind: 'mode', mode: 'agent' },
    ])
    // pin elements carry data-dock-pin
    const pins = container.querySelectorAll('[data-dock-pin]')
    expect(pins.length).toBe(1)
    expect(pins[0].getAttribute('data-sortable-id')).toBe('space-space-1')
  })

  it('renders the section divider between the last contiguous mode and the first non-mode', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'pinned-workspace', spaceId: 'space-1' },
      { kind: 'pinned-workspace', spaceId: 'space-2' },
    ])
    const divider = container.querySelector('[data-dock-section-divider]')
    expect(divider).not.toBeNull()
    // Divider sits in DOM order between Agent button and first pin tile.
    const buttons = container.querySelectorAll('button')
    const dividerEl = divider as HTMLElement
    const dividerPos = Array.from(container.querySelectorAll('button, [data-dock-section-divider]')).indexOf(dividerEl)
    const agentPos = Array.from(container.querySelectorAll('button, [data-dock-section-divider]')).indexOf(buttons[1])
    const firstPinPos = Array.from(container.querySelectorAll('button, [data-dock-section-divider]')).indexOf(buttons[2])
    expect(dividerPos).toBeGreaterThan(agentPos)
    expect(dividerPos).toBeLessThan(firstPinPos)
  })

  it('omits the divider entirely when no pinned entries are present', () => {
    const { container } = renderDockWithOrder([
      { kind: 'mode', mode: 'chat' },
      { kind: 'mode', mode: 'agent' },
      { kind: 'mode', mode: 'memory' },
      { kind: 'mode', mode: 'kaleidoscope' },
    ])
    expect(container.querySelector('[data-dock-section-divider]')).toBeNull()
  })
```

- [ ] **Step 2: Run, expect FAIL**

```bash
npm test -- --run src/components/dock/BottomDock.test.tsx
```

Expected: 3 new tests FAIL.

- [ ] **Step 3: Implement — BottomDock dispatch**

Edit `ui/src/components/dock/BottomDock.tsx`. Add the import:

```tsx
import { DockPinnedItem } from './DockPinnedItem'
```

Replace the existing `dockOrder.map((spec, i) => { if (spec.kind !== 'mode') return null ...})` block AND the inline static divider `<div className="mx-2 h-7 w-px ... bg-border/50" />` with a single render pass that interleaves items + divider:

```tsx
{(() => {
  // Find the first non-mode index — divider sits before it. If no
  // non-mode entries exist, no divider renders.
  const firstPinIdx = dockOrder.findIndex((s) => s.kind !== 'mode')
  return dockOrder.map((spec, i) => {
    const sortableId = specToSortableId(spec)
    // Insert divider just before the first pin.
    const dividerBefore = firstPinIdx !== -1 && i === firstPinIdx ? (
      <div
        key="dock-section-divider"
        data-dock-section-divider
        className="mx-2 h-7 w-px self-center bg-border/50"
        aria-hidden="true"
      />
    ) : null

    let body: React.ReactElement | null = null
    switch (spec.kind) {
      case 'mode': {
        const meta = MODE_REGISTRY[spec.mode]
        body = (
          <DockItem
            key={sortableId}
            sortableId={sortableId}
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
      case 'pinned-conversation':
        body = (
          <DockPinnedItem
            key={sortableId}
            sortableId={sortableId}
            label={`Conversation ${spec.sessionId.slice(0, 6)}`}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={setHoveredIndex}
            onClick={() => {
              // Phase 2B: programmatic-only. Phase 2D wires this to
              // useOpenSession to resume the actual conversation.
            }}
          />
        )
        break
      case 'pinned-workspace':
        body = (
          <DockPinnedItem
            key={sortableId}
            sortableId={sortableId}
            label={`Workspace ${spec.spaceId.slice(0, 6)}`}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={setHoveredIndex}
            onClick={() => {
              // Phase 2B: programmatic-only. Phase 2D wires this to
              // setActiveWorkspaceId to switch context.
            }}
          />
        )
        break
      case 'pinned-automation':
        body = (
          <DockPinnedItem
            key={sortableId}
            sortableId={sortableId}
            label={`Automation ${spec.specId.slice(0, 6)}`}
            index={i}
            hoveredIndex={hoveredIndex}
            onHoverIndexChange={setHoveredIndex}
            onClick={() => {
              // Phase 2B: programmatic-only. Phase 2D wires this to
              // the automation hub.
            }}
          />
        )
        break
    }

    return (
      <React.Fragment key={sortableId}>
        {dividerBefore}
        {body}
      </React.Fragment>
    )
  })
})()}
```

(The IIFE is a small concession to keep the divider-placement logic adjacent to the map — extracting to a top-level helper would add prop ferrying.)

ALSO REMOVE the existing inline divider that was always rendered:

```tsx
<div
  className="mx-2 h-7 w-px self-center bg-border/50"
  aria-hidden="true"
/>
```

The new divider is dynamic and lives inside the loop.

- [ ] **Step 4: Run, expect PASS**

```bash
npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts
```

Expected: all PASS (49 + 5 DockPinnedItem + 10 atoms + 3 BottomDock = 67).

- [ ] **Step 5: Type-check**

```bash
npx tsc --noEmit 2>&1 | grep -E "BottomDock" | head
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b
git add ui/src/components/dock/BottomDock.tsx ui/src/components/dock/BottomDock.test.tsx
git commit -m "feat(dock): dispatch pinned-* variants + dynamic section divider

Replace the 'skip non-mode' guard with a switch dispatching to either
DockItem (modes) or DockPinnedItem (pins). The cosmetic divider now
sits inside the loop at the first non-mode index — reordering a mode
past a pin moves the divider with it, so the visual section is always
derived from the actual kind boundary (per spec §2.3 'divider follows
kinds').

Pinned-conversation/workspace/automation onClick handlers are no-ops
in Phase 2B (programmatic-only). Phase 2D will wire them to
useOpenSession / setActiveWorkspaceId / automation-hub.

Labels render as 'Conversation/Workspace/Automation \${shortId}' until
Phase 2D pulls live titles from conversationsAtom / workspacesAtom /
automationSpecsAtom.

Phase 2B task 3 of 4."
```

---

## Task 4 · Verification + PR

Final polish and PR.

- [ ] **Step 1: Full Vitest suite**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b/ui
npm test -- --run 2>&1 | tail -6
```

Expected: ~860 pass / 10 baseline fail (unchanged).

- [ ] **Step 2: TypeScript check**

```bash
npx tsc --noEmit 2>&1 | head -10
```

Expected: only pre-existing baseline errors.

- [ ] **Step 3: Manual smoke test (browser console-driven, since no UI to add pins yet)**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b/src-tauri
cargo tauri dev
```

In the dev runtime, open the dev console (Cmd-Opt-I) and seed a pin programmatically. Inside the console, the Jotai store isn't directly accessible, but you can edit localStorage and reload:

```js
localStorage.setItem('dock:order', JSON.stringify([
  { kind: 'mode', mode: 'chat' },
  { kind: 'mode', mode: 'agent' },
  { kind: 'mode', mode: 'memory' },
  { kind: 'mode', mode: 'kaleidoscope' },
  { kind: 'pinned-workspace', spaceId: 'demo-1' },
  { kind: 'pinned-conversation', sessionId: 'demo-conv', type: 'agent' },
]))
location.reload()
```

Verify:
- Dock shows 4 modes + divider + 2 pinned squircles
- Each pin has a unique deterministic gradient (workspace = one color, conversation = different)
- Long-press a pin → drag it past a mode → release → divider follows the kind boundary
- Refresh — order persists

- [ ] **Step 4: Push branch + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex-p2b
git fetch origin main
git status -sb
git push -u origin claude/bottom-dock-apex-phase2b
```

If behind main, rebase first.

Open PR:

```bash
gh pr create --base main --head claude/bottom-dock-apex-phase2b \
  --title "feat(dock): apex polish Phase 2B — pin rendering (CSS squircle + divider)" \
  --body "$(cat <<'EOF'
## Summary

Phase 2B of the BottomDock apex polish series — pin rendering layer. Second of three Phase-2 sub-PRs (2A reorder ✅ → 2B pin render → 2C bounce).

- **DockPinnedItem** (new): CSS squircle (no PNG asset) with a deterministic gradient from `pinIdColorSeed` + glyph (emoji or first letter of label). Mirrors DockItem's magnification + drag composition exactly.
- **Helpers** (new): \`addDockPin\` / \`removeDockPin\` (idempotent, referential-equality no-op on collision) + \`pinIdColorSeed\` (FNV-1a hash → HSL gradient).
- **BottomDock dispatch**: switch on \`spec.kind\` routing modes to \`DockItem\` and pinned-* to \`DockPinnedItem\`. The cosmetic section divider now floats at the first non-mode index — reordering a mode past a pin moves the divider with it (per spec §2.3 "divider follows kinds").
- **No source UI yet**: right-click → Pin context menus on WorkspaceRail / chat / agent / automation lists are intentionally deferred to **Phase 2D** (or later). Pins can be added today via \`localStorage.setItem('dock:order', ...)\` + reload, or programmatically via the new helpers.

Spec: \`docs/superpowers/specs/2026-05-19-bottom-dock-apex-design.md\` §2.3
Plan: \`docs/superpowers/plans/2026-05-19-bottom-dock-apex-phase2b-pin-render.md\`

## Commits (bisectable)

| # | Commit | What |
|---|---|---|
| 1 | addDockPin + removeDockPin + pinIdColorSeed | pure helpers + 10 tests |
| 2 | DockPinnedItem component | CSS squircle with gradient + glyph |
| 3 | BottomDock dispatch + dynamic divider | switch render + kind-boundary divider |

## Test plan

- [x] \`npm test -- --run src/components/dock src/atoms/dock-atoms.test.ts\` — 67/67 pass (up from 49)
- [x] \`npx tsc --noEmit\` clean
- [ ] Manual: seed pins via \`localStorage.setItem('dock:order', JSON.stringify([...]))\` + reload, confirm pins render with unique gradients, divider sits between modes and pins, drag-reorder still works across the divider

## Follow-ups (tracked, not blockers)

- **Phase 2D · Source UIs**: right-click → Pin on WorkspaceRail sessions + workspace switcher + automation hub + (a yet-to-be-located) chat session surface. Phase 2D will also wire the onClick handlers in BottomDock's pin dispatch to \`useOpenSession\` / \`setActiveWorkspaceId\` / automation hub, and pull live titles/emojis from the source atoms.
- **Pin click-through**: today the pinned items' onClick is a no-op. Phase 2D wires it.
- **Soft cap of 8 pins / overflow popover**: deferred to Phase 2D.
- **Active-state indicator for pinned items**: deferred (modes have it; pins don't yet).
EOF
)"
```

---

## Out of scope (Phase 2D)

- Right-click → Pin on 4 source surfaces (WorkspaceRail / WorkspaceSwitcher / chat surface / AutomationHub)
- Pin click-through (\`useOpenSession\` / \`setActiveWorkspaceId\` / automation hub wiring)
- Live title/emoji resolution from \`conversationsAtom\` / \`workspacesAtom\` / \`automationSpecsAtom\`
- Unpin UX (right-click on pinned dock item → Unpin)
- Soft cap of 8 pins + overflow popover

These are the natural Phase 2D scope.

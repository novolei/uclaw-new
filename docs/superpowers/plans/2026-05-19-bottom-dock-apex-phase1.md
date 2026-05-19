# Bottom Dock — Apex Polish · Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Phase 1 of the BottomDock apex polish — swap the 4 lucide nav-icons for the V10 Liquid Glass PNG asset set, remove slot decoration in favor of a bottom indicator dot, bump display size to 28 px, add a drag-handle preview affordance, and replace the 3-dot connection indicator with a 3-bar signal-strength one.

**Architecture:** Phase 1 is *presentational* — no new atoms, no IPC events, no dependencies. Each task touches a single existing dock component, replaces its visual implementation, and updates the colocated Vitest spec. The data model is unchanged: `BottomDock` still iterates a hardcoded 4-item `NAV_ITEMS`; `DockItem` still receives an `icon` ReactNode (now a `<img>`) and renders it. Phase 2/3 will revisit the data model.

**Tech Stack:** React 18 + TypeScript, Vite (asset import via `?url`), Tailwind (semantic theme tokens), `motion/react`, Radix Tooltip, Vitest + React Testing Library + jsdom.

**Worktree:** `.claude/worktrees/dock-apex/` on branch `claude/bottom-dock-apex-phase1` (already created; commit `52e5dea` lands the spec + icon assets).

**Verification cadence:**
- After every task, run `cd ui && npm test -- --run src/components/dock` — all dock tests must pass.
- After every task, run `cd ui && npx tsc --noEmit` — type-clean.
- After the last task, run the full suite `cd ui && npm test -- --run` — no regressions beyond the 10 pre-existing baseline failures (kaleidoscope module count, SearchPalette FTS, IntelligenceTab, Memory module — all unrelated to dock).
- Final visual check: `cargo tauri dev` from `src-tauri/`, enable dock in Settings, hover to reveal, cycle themes (warm-paper, qingye, forest-evergreen) to confirm no hardcoded colors leak through.

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `ui/src/components/dock/BottomDock.tsx` | Swap lucide icons for `<img>` of PNG assets, update gap, keep all hover-reveal wiring |
| Modify | `ui/src/components/dock/DockItem.tsx` | Drop slot bg pill + ring + glow; render icon directly; bump SLOT_W/ICON_BOX; refine bottom dot |
| Modify | `ui/src/components/dock/DockItem.test.tsx` | Add assertion for "no slot decoration"; loosen `icon` prop type expectations |
| Create | `ui/src/components/dock/DockDragHandle.tsx` | 4 horizontal dots, 4 px ea, fade in on parent hover |
| Create | `ui/src/components/dock/DockDragHandle.test.tsx` | Renders 4 dots, has correct ARIA, default opacity 0 |
| Modify | `ui/src/components/dock/ConnectionIndicator.tsx` | Rewrite as 3-bar signal-strength, keep aria/tooltip semantics |
| Modify | `ui/src/components/dock/ConnectionIndicator.test.tsx` | Update DOM queries from `rounded-full` dots to `rounded` bars; per-channel state assertions |
| Modify | `ui/src/components/dock/BottomDockHoverRegion.test.tsx` | Bump motion mock once for new `img` rendering path (only if test currently asserts on lucide `svg` count) |

Files NOT touched in Phase 1: `BottomDockHoverRegion.tsx`, `dock-atoms.ts`, `useConnectionStatus.ts`, `AppShell.tsx`. All Phase 1 visuals slot into the existing reveal/hide state machine.

---

## Task 1 · Replace lucide icons with PNG `<img>` imports

Switch the four dock items from inline lucide React components to imported PNG asset URLs. The assets already exist on this branch (committed in `52e5dea`).

**Files:**
- Modify: `ui/src/components/dock/BottomDock.tsx`
- Modify: `ui/src/components/dock/DockItem.test.tsx`

- [ ] **Step 1: Write a failing test** — assert that `BottomDock` renders an `<img>` (not an `<svg>`) for every nav item.

Open `ui/src/components/dock/BottomDock.tsx` first to confirm `aria-label` values per item (`聊天`, `Agent`, `记忆`, `万花筒`). Then create `ui/src/components/dock/BottomDock.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { render, screen } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
import { BottomDock } from './BottomDock'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue({}) }))
vi.mock('./useConnectionStatus', () => ({ useConnectionStatus: () => {} }))
vi.mock('motion/react', () => ({
  motion: {
    button: React.forwardRef<
      HTMLButtonElement,
      React.ComponentPropsWithoutRef<'button'> & { style?: unknown }
    >(({ style: _style, ...rest }, ref) => <button ref={ref} {...rest} />),
    div: ({ initial: _i, animate: _a, transition: _t, ...rest }: React.ComponentPropsWithoutRef<'div'> & Record<string, unknown>) =>
      <div {...rest} />,
  },
  useSpring: () => ({ set: vi.fn() }),
  useReducedMotion: () => true,
}))

function renderDock(enabled = true) {
  const store = createStore()
  store.set(bottomDockEnabledAtom, enabled)
  return render(
    <JotaiProvider store={store}>
      <BottomDock revealed />
    </JotaiProvider>,
  )
}

describe('BottomDock · icons', () => {
  it('renders an <img> for every dock item (not lucide svg)', () => {
    renderDock()
    for (const label of ['聊天', 'Agent', '记忆', '万花筒']) {
      const btn = screen.getByRole('button', { name: label })
      // The dock item's icon must be an img element now, not an svg.
      const img = btn.querySelector('img')
      const svg = btn.querySelector('svg')
      expect(img, `${label} should render an <img>`).not.toBeNull()
      expect(svg, `${label} must no longer render an <svg> icon`).toBeNull()
    }
  })

  it('image src points to the PNG asset bundled by Vite', () => {
    renderDock()
    const chatImg = screen
      .getByRole('button', { name: '聊天' })
      .querySelector('img') as HTMLImageElement
    expect(chatImg.src).toMatch(/chat\.png/)
  })
})
```

- [ ] **Step 2: Run the test, expect it to fail**

Run: `cd ui && npm test -- --run src/components/dock/BottomDock.test.tsx`
Expected: FAIL — `BottomDock` currently renders lucide `<svg>` elements; `btn.querySelector('img')` is null.

- [ ] **Step 3: Implement — swap lucide for PNG imports in `BottomDock.tsx`**

Replace lines 1-12 (imports) and lines 38-82 (`NAV_ITEMS`) of `ui/src/components/dock/BottomDock.tsx`:

```tsx
import * as React from 'react'
import { motion } from 'motion/react'
import { useAtomValue, useSetAtom } from 'jotai'
import { DockItem } from './DockItem'
import { ConnectionIndicator } from './ConnectionIndicator'
import { useConnectionStatus } from './useConnectionStatus'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { topLevelViewAtom, type TopLevelView } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import chatIcon from '@/assets/dock-icons/chat.png'
import agentIcon from '@/assets/dock-icons/agent.png'
import memoryIcon from '@/assets/dock-icons/memory.png'
import kaleidoscopeIcon from '@/assets/dock-icons/kaleidoscope.png'

// ... BottomDockProps / NavCtx / ActionCtx interfaces unchanged ...

interface NavItem {
  id: string
  iconSrc: string         // ← new: src for <img>
  iconAlt: string         // ← new: alt text for accessibility
  label: string
  isActive: (ctx: NavCtx) => boolean
  onClick: (ctx: ActionCtx) => void
}

const NAV_ITEMS: NavItem[] = [
  {
    id: 'chat',
    iconSrc: chatIcon,
    iconAlt: '聊天',
    label: '聊天',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'chat' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('chat')
      setTopLevelView('workspace')
    },
  },
  {
    id: 'agent',
    iconSrc: agentIcon,
    iconAlt: 'Agent',
    label: 'Agent',
    isActive: ({ appMode, topLevelView }) =>
      appMode === 'agent' && topLevelView === 'workspace',
    onClick: ({ setAppMode, setTopLevelView }) => {
      setAppMode('agent')
      setTopLevelView('workspace')
    },
  },
  {
    id: 'memory',
    iconSrc: memoryIcon,
    iconAlt: '记忆',
    label: '记忆',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule === 'memory',
    onClick: ({ setKaleidoscopeModule, setTopLevelView }) => {
      setKaleidoscopeModule('memory')
      setTopLevelView('kaleidoscope')
    },
  },
  {
    id: 'kaleidoscope',
    iconSrc: kaleidoscopeIcon,
    iconAlt: '万花筒',
    label: '万花筒',
    isActive: ({ topLevelView, kaleidoscopeModule }) =>
      topLevelView === 'kaleidoscope' && kaleidoscopeModule !== 'memory',
    onClick: ({ setTopLevelView }) => {
      setTopLevelView('kaleidoscope')
    },
  },
]
```

Then update the `NAV_ITEMS.map` block (around lines 146-157) to pass the `<img>` element as the `icon` prop:

```tsx
{NAV_ITEMS.map((item, i) => (
  <DockItem
    key={item.id}
    icon={
      <img
        src={item.iconSrc}
        alt={item.iconAlt}
        draggable={false}
        className="w-7 h-7 select-none pointer-events-none"
      />
    }
    label={item.label}
    isActive={item.isActive(navCtx)}
    index={i}
    hoveredIndex={hoveredIndex}
    onHoverIndexChange={setHoveredIndex}
    onClick={() => item.onClick(actionCtx)}
  />
))}
```

Note: `w-7 h-7` = 28 px (Tailwind's `7` token = `1.75rem` = 28 px). This is the new display size baked into Task 3, but it's already correct here because the icon size is decoupled from the slot size.

- [ ] **Step 4: Run the test, expect it to pass**

Run: `cd ui && npm test -- --run src/components/dock/BottomDock.test.tsx`
Expected: PASS — both new tests pass.

Also run the existing `DockItem.test.tsx` to confirm nothing regressed (it uses `<Bot size={18} />` from lucide as test fixture, not from BottomDock — should still work):

Run: `cd ui && npm test -- --run src/components/dock`
Expected: all dock tests PASS.

- [ ] **Step 5: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "(BottomDock|DockItem|dock-icons)" | head -10`
Expected: empty output (no errors related to our edits).

Note: if `npx tsc --noEmit` reports errors elsewhere (e.g. `useBrowserScreencast.test.tsx` from PR #226 baseline), ignore them — those are pre-existing on `origin/main`.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git add ui/src/components/dock/BottomDock.tsx ui/src/components/dock/BottomDock.test.tsx
git commit -m "feat(dock): swap lucide icons for Liquid Glass PNG assets

Replace the 4 lucide nav icons (MessageSquare/Bot/Brain/Sparkles) with
imports of the V10 Liquid Glass PNG asset set committed in 52e5dea.
NavItem gains iconSrc + iconAlt fields; the <img> element is passed as
the icon prop to DockItem (still a ReactNode contract, no DockItem
signature change). Image size set inline to w-7 h-7 (28px) per spec §1.3
— SLOT_W bump comes in Task 3.

New test file: BottomDock.test.tsx asserts every dock item renders an
<img> (not an svg) and that the bundler-emitted URL points at the
correct asset.

Phase 1 task 1 of 5."
```

---

## Task 2 · Remove slot decoration; refine active-state dot

The current `DockItem.tsx` wraps the icon in a `<span>` with a colored backplate (active = `bg-primary/12 ring-1 shadow-[…]`, inactive = `bg-foreground/[0.06] hover:bg-foreground/[0.10]`). The Liquid Glass icons have their own color identity — the backplate competes with them. Replace the backplate with a transparent passthrough and rely on the bottom indicator dot (already present but conservatively styled) for active state.

**Files:**
- Modify: `ui/src/components/dock/DockItem.tsx`
- Modify: `ui/src/components/dock/DockItem.test.tsx`

- [ ] **Step 1: Write failing tests** — add two assertions to `DockItem.test.tsx`: (a) no backplate styling on the icon wrapper, (b) the active-dot element exists when `isActive`.

Append these tests to `ui/src/components/dock/DockItem.test.tsx` inside the existing `describe('DockItem', ...)` block:

```tsx
  it('does NOT render a colored slot backplate around the icon', () => {
    // Phase 1 removes the wrapping <span> that previously held a primary-tinted
    // pill + ring + glow when active, and a foreground/[0.06] background
    // when inactive. The Liquid Glass icons have their own color identity;
    // a backplate would compete with them.
    const { container } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    // No element inside the button should carry the legacy backplate classes.
    const button = screen.getByRole('button', { name: 'Agent' })
    const html = button.outerHTML
    expect(html).not.toMatch(/bg-primary\/12/)
    expect(html).not.toMatch(/bg-foreground\/\[0\.06\]/)
    expect(html).not.toMatch(/ring-primary\/30/)
    // Don't assert a more general "no shadow" because the active dot can keep a glow.
    expect(container).toBeTruthy()
  })

  it('renders the active-state dot only when isActive', () => {
    const { container: activeContainer } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(activeContainer.querySelector('[data-dock-active-dot]')).not.toBeNull()

    const { container: inactiveContainer } = render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={1}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    expect(inactiveContainer.querySelector('[data-dock-active-dot]')).toBeNull()
  })
```

- [ ] **Step 2: Run the tests, expect them to fail**

Run: `cd ui && npm test -- --run src/components/dock/DockItem.test.tsx`
Expected: 2 new tests FAIL — current DOM has `bg-primary/12` for active, and the active dot has no `data-dock-active-dot` attribute yet.

- [ ] **Step 3: Implement — strip the backplate `<span>` and refine the dot**

Replace the JSX inside `<motion.button>` in `ui/src/components/dock/DockItem.tsx` (lines 91-111). The whole return statement becomes:

```tsx
  return (
    <TooltipProvider delayDuration={140} skipDelayDuration={80}>
      <Tooltip>
        <TooltipTrigger asChild>
          <motion.button
            type="button"
            className="relative flex items-end justify-center select-none outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-0 rounded-[14px]"
            style={{
              width: SLOT_W,
              height: SLOT_W,
              scale: scaleSpring,
              y: ySpring,
              transformOrigin: 'bottom center',
            }}
            onMouseEnter={() => onHoverIndexChange(index)}
            onMouseLeave={() => onHoverIndexChange(null)}
            onClick={onClick}
            aria-label={label}
            aria-pressed={isActive}
          >
            {/* Icon renders flush — no slot backplate. The Liquid Glass PNG
                IS the visual; an inner pill would compete with it. */}
            <span
              className="flex items-center justify-center"
              style={{ width: ICON_BOX, height: ICON_BOX }}
            >
              {icon}
            </span>
            {/* Active indicator — solid primary dot 8 px below icon, slight glow. */}
            {isActive && (
              <span
                data-dock-active-dot
                className="pointer-events-none absolute left-1/2 -translate-x-1/2 -bottom-1 w-1 h-1 rounded-full bg-primary shadow-[0_0_6px_hsl(var(--primary)/0.5)]"
                aria-hidden="true"
              />
            )}
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
```

Key differences vs. the current implementation:
- The inner `<span>` no longer carries `bg-primary/12 ring-1 ring-primary/30 shadow-[…]` (active) or `bg-foreground/[0.06] hover:bg-foreground/[0.10]` (inactive) — just sizing.
- `cn` import is no longer needed in this file (the conditional className was the only consumer). Remove the import: replace line 3 `import { cn } from '@/lib/utils'` with nothing.
- Active dot uses solid render-or-not (`{isActive && …}`) instead of an always-rendered `<span>` with width/opacity toggle — simpler DOM and matches the test's `querySelector` expectation.
- Active dot is now `w-1 h-1` (4×4 px solid), matches spec §1.2.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `cd ui && npm test -- --run src/components/dock/DockItem.test.tsx`
Expected: all 6 tests in `DockItem.test.tsx` PASS (4 original + 2 new).

Also run sibling tests to confirm no regression:
Run: `cd ui && npm test -- --run src/components/dock`
Expected: all dock tests PASS.

- [ ] **Step 5: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "DockItem" | head -5`
Expected: empty (no DockItem-related errors).

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git add ui/src/components/dock/DockItem.tsx ui/src/components/dock/DockItem.test.tsx
git commit -m "feat(dock): remove slot decoration; active = bottom dot only

The Liquid Glass icons (Task 1) carry their own permanent color identity
(Music-style indigo, sky-blue, etc.). The previous slot backplate
(bg-primary/12 + ring + shadow on active, bg-foreground/[0.06] +
hover:bg-foreground/[0.10] on rest) competed with that visual.

Remove the inner backplate <span> entirely. Active state now indicated
by a single 4x4 px primary-tinted dot 4 px below the icon (data attr
data-dock-active-dot for test addressability), with a soft glow.

Spec §1.2.

Phase 1 task 2 of 5."
```

---

## Task 3 · Bump dock display size to 28 px

The Liquid Glass icons demand a larger render to show their detail. Bump `SLOT_W` 48 → 56 and `ICON_BOX` 38 → 44 in `DockItem.tsx`. The icon img itself is already 28 px from Task 1.

**Files:**
- Modify: `ui/src/components/dock/DockItem.tsx`
- Modify: `ui/src/components/dock/DockItem.test.tsx`

- [ ] **Step 1: Write failing test** — assert the button's inline width is 56 px.

Append to the `describe('DockItem', …)` block in `DockItem.test.tsx`:

```tsx
  it('renders a 56 px slot (28 px icon + breathing room)', () => {
    render(
      <DockItem
        icon={<Bot size={18} />}
        label="Agent"
        isActive={false}
        index={0}
        hoveredIndex={null}
        onHoverIndexChange={vi.fn()}
        onClick={vi.fn()}
      />,
    )
    const btn = screen.getByRole('button', { name: 'Agent' })
    // SLOT_W is applied via inline style; jsdom exposes it on style.width.
    expect(btn.style.width).toBe('56px')
    expect(btn.style.height).toBe('56px')
  })
```

- [ ] **Step 2: Run, expect fail**

Run: `cd ui && npm test -- --run src/components/dock/DockItem.test.tsx`
Expected: the new test FAILS with `expected '48px' to be '56px'`.

- [ ] **Step 3: Implement — bump the constants**

Edit `ui/src/components/dock/DockItem.tsx` lines 31-32:

```tsx
const SLOT_W = 56 // px, holds 44 px ICON_BOX comfortably even at hover scale 1.34
const ICON_BOX = 44 // px
```

That's the only change in this task.

- [ ] **Step 4: Run, expect pass**

Run: `cd ui && npm test -- --run src/components/dock/DockItem.test.tsx`
Expected: all DockItem tests PASS, including the new 56 px assertion.

- [ ] **Step 5: Type-check + full dock-tests sweep**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep DockItem`
Expected: empty.

Run: `cd ui && npm test -- --run src/components/dock`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git add ui/src/components/dock/DockItem.tsx ui/src/components/dock/DockItem.test.tsx
git commit -m "feat(dock): bump slot to 56 px / icon box to 44 px / display 28 px

The Liquid Glass icons need more pixels to show their detail. Bump:
  SLOT_W:   48 → 56
  ICON_BOX: 38 → 44

Image rendered size stays at 28 px (set inline as w-7 h-7 in Task 1).
The 28 px image inside a 44 px box gives 8 px of margin on each side,
which lets the active dot sit cleanly below without being clipped.

Total dock width grows ~10 px per item (~40 px overall for 4 items) —
well within macOS Dock comfort proportions.

Spec §1.3.

Phase 1 task 3 of 5."
```

---

## Task 4 · Drag handle preview (4 dots, hover-fade)

Add a non-interactive visual affordance hinting that the dock supports reorder/pin. Phase 2 wires the actual drag behavior; Phase 1 only ships the affordance.

**Files:**
- Create: `ui/src/components/dock/DockDragHandle.tsx`
- Create: `ui/src/components/dock/DockDragHandle.test.tsx`
- Modify: `ui/src/components/dock/BottomDock.tsx` (mount the handle)

- [ ] **Step 1: Write failing test** — create `ui/src/components/dock/DockDragHandle.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { DockDragHandle } from './DockDragHandle'

describe('DockDragHandle', () => {
  it('renders 4 dots inside a role=presentation wrapper', () => {
    render(<DockDragHandle />)
    const handle = screen.getByRole('presentation', { name: /drag/i })
    const dots = handle.querySelectorAll('span')
    expect(dots.length).toBe(4)
  })

  it('is hidden by default (opacity 0 via data-state)', () => {
    render(<DockDragHandle />)
    const handle = screen.getByRole('presentation', { name: /drag/i })
    // Handle is opacity:0 by default; CSS sibling/parent hover will fade it in.
    // We assert via a data attribute so JSDOM can verify without a real hover.
    expect(handle.getAttribute('data-state')).toBe('idle')
  })
})
```

- [ ] **Step 2: Run, expect fail**

Run: `cd ui && npm test -- --run src/components/dock/DockDragHandle.test.tsx`
Expected: FAIL with module-not-found error.

- [ ] **Step 3: Implement DockDragHandle**

Create `ui/src/components/dock/DockDragHandle.tsx`:

```tsx
import * as React from 'react'

/**
 * Visual-only affordance hinting that the dock supports reorder / pinning.
 * Renders four small dots centered above the dock body. Default opacity 0;
 * the dock's hover state (applied via the `group` modifier in BottomDock)
 * fades it to 0.55 over 150 ms.
 *
 * Phase 2 wires the actual drag behavior via dnd-kit; this component stays
 * presentational and decoupled from drag state.
 */
export function DockDragHandle(): React.ReactElement {
  return (
    <div
      role="presentation"
      aria-label="drag handle"
      data-state="idle"
      className="
        pointer-events-none absolute left-1/2 -translate-x-1/2 top-1
        flex items-center justify-center gap-[3px]
        opacity-0 group-hover:opacity-[0.55] transition-opacity duration-150
      "
    >
      <span className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span className="block w-1 h-1 rounded-full bg-foreground/45" />
      <span className="block w-1 h-1 rounded-full bg-foreground/45" />
    </div>
  )
}
```

Tailwind note: `group-hover:opacity-[0.55]` is Tailwind's arbitrary-value syntax — works out of the box, no config extension. The parent (the dock root in `BottomDock.tsx`) gets a `group` class in Step 5 below so this `group-hover:` modifier kicks in when the dock body is hovered.

- [ ] **Step 4: Run, expect first test to pass; second may still report mismatch**

Run: `cd ui && npm test -- --run src/components/dock/DockDragHandle.test.tsx`
Expected: both tests PASS.

If `getByRole('presentation', { name: /drag/i })` fails to find the element, the issue is jsdom's lookup of `name`. Use `screen.getByLabelText('drag handle')` instead in the test, or `screen.getByText` against the `aria-label`. Simpler fix: update the test to use:

```tsx
const handle = document.querySelector('[role="presentation"][aria-label="drag handle"]') as HTMLElement
expect(handle).not.toBeNull()
```

If you change the test, re-run.

- [ ] **Step 5: Mount the handle in BottomDock**

Edit `ui/src/components/dock/BottomDock.tsx`. Add the import next to the other dock imports near the top:

```tsx
import { DockDragHandle } from './DockDragHandle'
```

Then add the `group` class to the `motion.div` root and mount `<DockDragHandle />` as its first child. The current root element starts at line 137. Update the className and add the handle:

```tsx
    <motion.div
      role="navigation"
      aria-label="底部导航"
      className="group relative flex items-end gap-1 px-3 pt-3 pb-2 rounded-t-2xl bg-popover/85 backdrop-blur-xl border-t border-x border-border/40 shadow-[0_-10px_30px_-12px_rgba(0,0,0,0.35)] supports-[backdrop-filter]:bg-popover/70 will-change-transform"
      initial={false}
      animate={{ y: revealed ? 0 : SLIDE_HIDDEN_Y, opacity: revealed ? 1 : 0 }}
      transition={revealed ? REVEAL_TRANSITION : HIDE_TRANSITION}
      onMouseLeave={() => setHoveredIndex(null)}
    >
      <DockDragHandle />
      {NAV_ITEMS.map((item, i) => (
        // ... unchanged ...
```

Three changes to the existing `<motion.div>`:
- Class `group` added (enables `group-hover:` on the handle).
- Class `relative` added (positions the absolute handle relative to the dock body).
- `<DockDragHandle />` added as the first child.

- [ ] **Step 6: Verify BottomDock tests still pass**

Run: `cd ui && npm test -- --run src/components/dock`
Expected: all PASS. The Task 1 `BottomDock.test.tsx` tests don't look at the drag handle; the new `DockDragHandle.test.tsx` is the only new coverage.

- [ ] **Step 7: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "(DockDragHandle|BottomDock)" | head -5`
Expected: empty.

- [ ] **Step 8: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git add ui/src/components/dock/DockDragHandle.tsx ui/src/components/dock/DockDragHandle.test.tsx ui/src/components/dock/BottomDock.tsx
git commit -m "feat(dock): drag-handle preview affordance (4 dots, hover-fade)

Add a visual-only affordance hinting the dock supports reorder / pinning
(both wired in Phase 2). 4 small dots (4 px each, 3 px gap) centered
above the dock body. Default opacity 0; the dock root gains a 'group'
class so the handle fades to ~0.55 over 150 ms on dock hover.

The component is decoupled from any drag state — it does not consume
atoms or events. Phase 2 will replace the static affordance with a
dnd-kit-driven version that also exposes a grab handle for kbd users.

Spec §1.4.

Phase 1 task 4 of 5."
```

---

## Task 5 · 3-bar signal-strength ConnectionIndicator

Replace the 3 sage/coral/amber circle dots with 3 vertical signal-strength bars (heights 6/10/14 px, 3 px wide, 2 px gap). Same backend channels (Internet / backend / memU), same tooltips, same a11y semantics — purely a visual swap.

**Files:**
- Modify: `ui/src/components/dock/ConnectionIndicator.tsx`
- Modify: `ui/src/components/dock/ConnectionIndicator.test.tsx`

- [ ] **Step 1: Write failing test** — refactor `ConnectionIndicator.test.tsx` to assert on bars.

Replace the existing content of `ui/src/components/dock/ConnectionIndicator.test.tsx` with:

```tsx
import * as React from 'react'
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { createStore, Provider as JotaiProvider } from 'jotai'
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
    </JotaiProvider>,
  )
}

describe('ConnectionIndicator', () => {
  it('renders the status container with aria-label', () => {
    renderWithStore({ internet: true, backend: true, memu: true })
    expect(screen.getByLabelText('连接状态')).toBeInTheDocument()
  })

  it('renders exactly 3 signal bars', () => {
    const { container } = renderWithStore({ internet: true, backend: true, memu: true })
    const bars = container.querySelectorAll('[data-conn-bar]')
    expect(bars.length).toBe(3)
  })

  it('marks an offline channel with data-state=offline', () => {
    const { container } = renderWithStore({ internet: false, backend: true, memu: true })
    const internetBar = container.querySelector('[data-conn-bar="internet"]')
    expect(internetBar?.getAttribute('data-state')).toBe('offline')
    // When internet is offline, backend/memu cascade to offline too.
    expect(container.querySelector('[data-conn-bar="backend"]')?.getAttribute('data-state')).toBe('offline')
    expect(container.querySelector('[data-conn-bar="memu"]')?.getAttribute('data-state')).toBe('offline')
  })

  it('marks memu warning state when memu atom is null (initializing)', () => {
    const { container } = renderWithStore({ internet: true, backend: true, memu: null })
    expect(container.querySelector('[data-conn-bar="memu"]')?.getAttribute('data-state')).toBe('warning')
  })
})
```

- [ ] **Step 2: Run, expect fail**

Run: `cd ui && npm test -- --run src/components/dock/ConnectionIndicator.test.tsx`
Expected: FAILs — current DOM has no `[data-conn-bar]` attribute; the indicator renders `rounded-full` dots.

- [ ] **Step 3: Implement — rewrite ConnectionIndicator as signal bars**

Replace the entire content of `ui/src/components/dock/ConnectionIndicator.tsx` with:

```tsx
import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
} from '@/atoms/dock-atoms'

type BarState = 'online' | 'warning' | 'offline'

interface SignalBarProps {
  channel: 'internet' | 'backend' | 'memu'
  state: BarState
  /** 6, 10, or 14 — the bar's height in px (signal-strength shape) */
  height: number
  tooltipText: string
}

function SignalBar({ channel, state, height, tooltipText }: SignalBarProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          data-conn-bar={channel}
          data-state={state}
          role="status"
          aria-label={tooltipText}
          style={{ height }}
          className={cn(
            'block w-[3px] rounded-[1.5px] transition-colors duration-200',
            state === 'online' && 'bg-sage-500 shadow-[0_0_3px_-1px_theme(colors.sage.500)]',
            state === 'warning' && 'bg-amber-500',
            state === 'offline' && 'bg-coral-500',
          )}
        />
      </TooltipTrigger>
      <TooltipContent
        side="top"
        sideOffset={6}
        className="text-[11px] px-2 py-1 rounded-md bg-popover/95 text-popover-foreground border border-border/60 shadow-md"
      >
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  )
}

export function ConnectionIndicator() {
  const internet = useAtomValue(internetOnlineAtom)
  const backend = useAtomValue(backendOnlineAtom)
  const memu = useAtomValue(memuOnlineAtom)

  const netState: BarState = internet ? 'online' : 'offline'
  const backendState: BarState = !internet
    ? 'offline'
    : backend
      ? 'online'
      : 'offline'
  const memuState: BarState = !internet
    ? 'offline'
    : memu === null
      ? 'warning'
      : memu
        ? 'online'
        : 'offline'

  return (
    <TooltipProvider delayDuration={220}>
      <div
        className="flex items-end gap-[2px] h-[18px]"
        aria-label="连接状态"
        role="group"
      >
        <SignalBar
          channel="internet"
          state={netState}
          height={6}
          tooltipText={`网络：${internet ? '在线' : '离线'}`}
        />
        <SignalBar
          channel="backend"
          state={backendState}
          height={10}
          tooltipText={`后端：${!internet ? '离线' : backend ? '在线' : '离线'}`}
        />
        <SignalBar
          channel="memu"
          state={memuState}
          height={14}
          tooltipText={`memU：${!internet ? '离线' : memu === null ? '初始化中' : memu ? '在线' : '离线'}`}
        />
      </div>
    </TooltipProvider>
  )
}
```

Behavior is identical to the dot version — same per-channel cascading logic, same tooltips. Visual swap only.

- [ ] **Step 4: Run, expect pass**

Run: `cd ui && npm test -- --run src/components/dock/ConnectionIndicator.test.tsx`
Expected: all 4 tests PASS.

Full dock sweep:
Run: `cd ui && npm test -- --run src/components/dock`
Expected: all dock tests PASS.

- [ ] **Step 5: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep ConnectionIndicator`
Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git add ui/src/components/dock/ConnectionIndicator.tsx ui/src/components/dock/ConnectionIndicator.test.tsx
git commit -m "feat(dock): 3-bar signal-strength connection indicator

Replace the 3 sage/coral/amber circle dots with 3 vertical signal-
strength bars (heights 6/10/14 px, 3 px wide, 2 px gap). Behaviorally
identical — same channels (Internet / backend / memU), same cascading
offline-when-internet-down logic, same Radix Tooltip texts. Purely a
visual upgrade per spec §1.5.

Each bar carries data-conn-bar={channel} and data-state={online|warning|
offline} for test addressability + future styling hooks (Phase 3 may
animate the memU bar during consolidation).

Spec §1.5.

Phase 1 task 5 of 5."
```

---

## Task 6 · Verification + PR shape

The implementation is done after Task 5. This task is the closing checks the engineer should run before opening the PR.

**Files:** none — verification only.

- [ ] **Step 1: Full Vitest suite**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex/ui
npm test -- --run 2>&1 | tail -10
```

Expected output line: `Tests  XX passed | 10 failed` where:
- The 10 failures are pre-existing on `origin/main` (kaleidoscope module count, SearchPalette FTS, IntelligenceTab, Memory module). They are unrelated to dock work and must NOT grow with our changes.
- If failures > 10, run `npm test -- --run 2>&1 | grep -E "FAIL|✗"` and surface the new ones in the PR body.

The dock-specific count should grow from 22 (pre-Phase-1) to 26+ tests (added 2 in DockItem + 2 in DockDragHandle + 2 net in ConnectionIndicator).

- [ ] **Step 2: Full TypeScript check**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex/ui
npx tsc --noEmit 2>&1 | head -10
```

Expected: any errors must be pre-existing (`useBrowserScreencast.test.tsx` and `useBrowserTaskEvents.test.tsx` are known baseline issues from PR #226). Errors mentioning `dock/`, `BottomDock`, `DockItem`, `DockDragHandle`, `ConnectionIndicator`, or `dock-icons` must be zero.

- [ ] **Step 3: Backend build (defensive — these are frontend-only changes but the spec lives in the repo so Rust should still compile)**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex/src-tauri
cargo build 2>&1 | grep -E "^error" | head
```

Expected: empty (no Rust errors).

- [ ] **Step 4: Manual visual check**

Start the dev runtime:

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex/src-tauri
cargo tauri dev
```

Open Settings → 通用 → enable the bottom dock toggle. Hover the bottom center of the window to reveal. Verify:

- Four icons render as PNGs at 28 px (Liquid Glass color identities visible: chat=sky-cyan, agent=indigo-violet, memory=amber-rose, kaleidoscope=violet-pink).
- No primary-tinted pill or ring around the active icon. A single small primary dot sits below.
- Hovering brings up tooltips with the Chinese labels (聊天 / Agent / 记忆 / 万花筒).
- Hovering the dock body brings up the 4-dot drag handle at the top-center, fading in over ~150 ms. It fades out on leave.
- The right side of the dock shows 3 vertical bars in sage green (assuming all connections are healthy). Hover each bar → tooltip naming the channel + state.
- Cycle themes via Settings → 主题 → warm-paper, qingye, forest-evergreen, forest-spring. Confirm the dock surface, tooltip text, drag handle, and signal bars all remain readable (no hardcoded colors should leak through; the icons themselves intentionally keep their brand color across all themes per spec §1.1).
- Trigger the hide animation by moving the cursor away. Confirm the soft "slip away" transition from PR #227 still works (no slam, no rebound).

- [ ] **Step 5: Push branch + open PR**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex
git fetch origin main
git status -sb
git push -u origin claude/bottom-dock-apex-phase1
```

If `git status -sb` reports `[behind N]`, rebase onto latest main first: `git rebase origin/main` and re-run `npm test -- --run src/components/dock` to confirm tests still pass post-rebase.

Then create the PR with `gh pr create`:

```bash
gh pr create --base main --head claude/bottom-dock-apex-phase1 \
  --title "feat(dock): apex polish Phase 1 — Liquid Glass icons + 28 px + signal bars" \
  --body "$(cat <<'EOF'
## Summary

Phase 1 of the BottomDock apex polish series — the visual layer.

- **9-icon Liquid Glass asset set** committed at \`ui/src/assets/dock-icons/*.png\` (1024×1024 RGBA transparent PNGs, generated via Gemini 2.5 Flash Image (Nano Banana) with the Apple Music macOS icon as style reference). Phase 1 wires the 4 dock icons; the other 5 (connections / home-office / humane / alert / settings) are committed for Phase 2 and beyond.
- **Slot decoration removed**: the inner primary-tinted pill + ring + shadow was competing with the icons' own color identity. Active state now indicated by a single 4 px primary dot below the icon.
- **28 px display size** (SLOT_W 48 → 56, ICON_BOX 38 → 44, icon img 28×28). The Liquid Glass detail demands the extra pixels.
- **Drag handle preview** (4 dots top-center, fades in on dock hover) — visual affordance for Phase 2 reorder/pin.
- **3-bar signal-strength connection indicator** replaces the 3 sage/coral/amber dots. Same backend channels, same tooltips.

Spec: \`docs/superpowers/specs/2026-05-19-bottom-dock-apex-design.md\`
Plan: \`docs/superpowers/plans/2026-05-19-bottom-dock-apex-phase1.md\`

## Commits (bisectable)

| # | Task | Commit |
|---|---|---|
| 1 | Spec + 9 icon assets | \`52e5dea\` |
| 2 | Swap lucide → PNG assets | \`feat(dock): swap lucide icons for Liquid Glass PNG assets\` |
| 3 | Remove slot decoration + active dot | \`feat(dock): remove slot decoration; active = bottom dot only\` |
| 4 | Bump display size to 28 px | \`feat(dock): bump slot to 56 px / icon box to 44 px / display 28 px\` |
| 5 | Drag handle preview | \`feat(dock): drag-handle preview affordance (4 dots, hover-fade)\` |
| 6 | 3-bar connection indicator | \`feat(dock): 3-bar signal-strength connection indicator\` |

## Test plan

- [x] \`npm test -- --run src/components/dock\` — 26+/26+ pass (was 22)
- [x] \`npx tsc --noEmit\` — clean for dock files
- [x] Full Vitest: no new failures (10 pre-existing baseline preserved)
- [ ] Manual: open \`cargo tauri dev\`, enable dock, verify Liquid Glass icons render at 28 px, no slot pill, active dot visible
- [ ] Manual: hover dock body — drag handle fades in
- [ ] Manual: cycle warm-paper / qingye / forest-evergreen themes — surface + tooltip + handle + bars stay readable
- [ ] Manual: soft-hide transition from PR #227 still smooth
EOF
)"
```

- [ ] **Step 6: Cleanup**

After the PR merges (presumably with `--merge` per the project style), sync local main:

```bash
cd /Users/ryanliu/Documents/uclaw
git fetch origin main:main  # if parent is not on main
# OR
git pull --ff-only origin main  # if parent IS on main
```

Cleanup worktree if desired:

```bash
rm /Users/ryanliu/Documents/uclaw/.claude/worktrees/dock-apex/ui/node_modules  # symlink
git worktree remove .claude/worktrees/dock-apex
git branch -d claude/bottom-dock-apex-phase1
git push origin --delete claude/bottom-dock-apex-phase1
```

Phase 2 will start from a fresh worktree on a new branch (`claude/bottom-dock-apex-phase2`), branching from updated `main`.

---

## Out of scope (Phase 2 / Phase 3 reminder)

Phase 1 explicitly does NOT do:
- Drag-to-reorder behavior (Phase 2 — `@dnd-kit/*` integration on top of the handle from Task 4)
- Pin to Dock (Phase 2 — new `dock-atoms` data model)
- Bounce on event (Phase 2 — auto-reveal + spring bounce + event-bus subscriptions)
- Agent breathing ring / streaming particles / memory pulse (Phase 3 — `useDockLiveness` hook + IPC event for memU consolidation)
- Webp variant emission at build time (Phase 1.5 — Vite plugin; not blocking)

Each will get its own plan file in the same `docs/superpowers/plans/` directory.

# HomeOffice Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a full-screen "Sky Island" HomeOffice view: PixiJS 2.5D scene with 8 zones, one Lofi-girl character walking between zones in response to agent IPC events, plus 4 interactive zone modals (music / sticky / diary / garden→skills). Entry button under "Automations" in LeftSidebar. Zero backend changes.

**Architecture:** PixiJS v8 + `@pixi/react` v8 declarative scene. State held in jotai atoms; agent state machine driven by reused Tauri events (`chat:stream-*`, `agent:stream-reset`). Character is an animated WebP whose frames are decoded via `ImageDecoder` API into `PIXI.Texture[]` and swapped per-frame by a custom `WebpAnimator`. The view replaces the chat/welcome body in `MainArea` (same pattern as AutomationHub), so LeftSidebar + RightChatPanel stay visible.

**Tech Stack:** React 18 + TypeScript + Jotai + PixiJS v8 + @pixi/react v8 + animated WebP + Vitest + React Testing Library + jsdom. Existing Tauri events: `chat:stream-chunk` / `chat:stream-tool-activity` / `chat:stream-complete` / `chat:stream-error` / `agent:stream-reset`.

**Spec source of truth:** [docs/superpowers/specs/2026-05-14-home-office-design.md](../specs/2026-05-14-home-office-design.md). Zone coordinates, state-machine table, asset list, and decision log live there; this plan implements them.

**Honest commit count:** ~18 commits. One PR with bisectable commits per CLAUDE.md PR shape.

**Asset note:** This plan uses **placeholder sprites** (solid-color rectangles with text label) for character WebPs during code development. Real Veo-generated Lofi-girl assets are produced in Task 18 (script) and verified in Task 19. The placeholder path keeps code tasks decoupled from the Veo pipeline so an engineer can implement and test the full scene without running gcloud.

---

## File Structure Overview

**New files (24):**

```
ui/public/home-office/                                    [CREATE — assets dir]
├── scene-sky-v5.png                                      [COPY from brainstorm renders]
├── sprites/lofi-girl/
│   ├── walk-N.webp  walk-NW.webp  walk-W.webp  walk-SW.webp  walk-S.webp
│   ├── pose-idle.webp  pose-thinking.webp  pose-typing.webp
│   └── pose-success.webp  pose-error.webp                [Task 19 — real assets]
│   └── _placeholder.png                                  [Task 1 — code dev]
└── audio/lofi-placeholder.mp3                            [Task 13 — CC0 occlude]

ui/src/atoms/
├── home-office-atoms.ts                                  [CREATE — all atoms]
└── home-office-atoms.test.ts                             [CREATE]

ui/src/components/home-office/
├── HomeOfficeView.tsx                                    [CREATE — page container]
├── HomeOfficeView.test.tsx                               [CREATE]
├── scene/
│   ├── HomeOfficeScene.tsx                               [CREATE — pixi-react Stage]
│   ├── hit-areas.ts                                      [CREATE — zone constants]
│   ├── dir-utils.ts                                      [CREATE — vec→8-dir + mirror]
│   ├── dir-utils.test.ts                                 [CREATE]
│   ├── sprite-loader.ts                                  [CREATE — WebP→Texture[]]
│   ├── animator.ts                                       [CREATE — WebpAnimator]
│   ├── animator.test.ts                                  [CREATE]
│   └── layers/
│       ├── BackgroundLayer.tsx                           [CREATE]
│       ├── ZoneLayer.tsx                                 [CREATE]
│       ├── CharacterLayer.tsx                            [CREATE]
│       └── ParticleLayer.tsx                             [CREATE]
└── zones/
    ├── MusicGazeboModal.tsx                              [CREATE]
    ├── StickyNoteModal.tsx                               [CREATE]
    └── DiaryDeskModal.tsx                                [CREATE]

ui/src/hooks/
├── useCharacterPath.ts                                   [CREATE — lerp + dir selection]
├── useCharacterPath.test.ts                              [CREATE]
├── useHomeOfficeAgentSync.ts                             [CREATE — IPC→state]
└── useHomeOfficeAgentSync.test.ts                        [CREATE]

scripts/
└── gen-home-office-sprites.sh                            [CREATE — Veo helper]
```

**Files to modify (4):**

- `ui/src/atoms/index.ts` — re-export `home-office-atoms`
- `ui/src/components/app-shell/LeftSidebar.tsx` — add "Home Office" button under Automations (line ~1064)
- `ui/src/components/tabs/MainArea.tsx` — render `<HomeOfficeView />` when `homeOfficePanelOpenAtom` is true (mirror AutomationHub pattern at line ~146–164)
- `package.json` (`ui/package.json`) — add `pixi.js` + `@pixi/react` deps

---

## Task 1: Install PixiJS dependencies + scaffolding

**Files:**
- Modify: `ui/package.json`
- Create: `ui/public/home-office/.gitkeep`
- Create: `ui/public/home-office/sprites/lofi-girl/.gitkeep`
- Create: `ui/src/components/home-office/.gitkeep`
- Create: `ui/src/components/home-office/scene/layers/.gitkeep`
- Create: `ui/src/components/home-office/zones/.gitkeep`

- [ ] **Step 1: Install pixi.js + @pixi/react**

```bash
cd ui && npm install pixi.js@^8 @pixi/react@^8
```

Expected: `package.json` gains both deps under `dependencies`; `package-lock.json` updated.

- [ ] **Step 2: Verify install + types resolve**

```bash
cd ui && node -e "console.log(require('pixi.js/package.json').version)"
cd ui && node -e "console.log(require('@pixi/react/package.json').version)"
```

Expected: prints `8.x.x` for both. If either fails, retry with explicit `npm install pixi.js@8.6.6 @pixi/react@8.0.0`.

- [ ] **Step 3: Create empty asset + component dirs**

```bash
mkdir -p ui/public/home-office/sprites/lofi-girl
mkdir -p ui/public/home-office/audio
mkdir -p ui/src/components/home-office/scene/layers
mkdir -p ui/src/components/home-office/zones
touch ui/public/home-office/.gitkeep
touch ui/public/home-office/sprites/lofi-girl/.gitkeep
touch ui/public/home-office/audio/.gitkeep
touch ui/src/components/home-office/.gitkeep
touch ui/src/components/home-office/scene/layers/.gitkeep
touch ui/src/components/home-office/zones/.gitkeep
```

- [ ] **Step 4: Copy v5 background scene**

```bash
cp .superpowers/brainstorm/24317-1778687442/renders/scene-sky-v5-rich.png \
   ui/public/home-office/scene-sky-v5.png
ls -la ui/public/home-office/scene-sky-v5.png
```

Expected: file exists, size > 200KB.

- [ ] **Step 5: Generate placeholder sprite for dev**

```bash
cd ui/public/home-office/sprites/lofi-girl
# 720x720 RGBA placeholder, pink rectangle with text "LOFI"
python3 -c "
from PIL import Image, ImageDraw, ImageFont
img = Image.new('RGBA', (720, 720), (0,0,0,0))
draw = ImageDraw.Draw(img)
draw.rectangle([180, 100, 540, 620], fill=(255, 192, 203, 255))
draw.text((280, 320), 'LOFI', fill=(80, 30, 50, 255))
img.save('_placeholder.png')
"
ls -la _placeholder.png
```

Expected: 720×720 PNG, ~5KB.

- [ ] **Step 6: Verify TypeScript still compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors (no new code yet, just deps).

- [ ] **Step 7: Commit**

```bash
git add ui/package.json ui/package-lock.json ui/public/home-office ui/src/components/home-office
git commit -m "feat(home-office): scaffold deps + asset dirs + v5 background"
```

---

## Task 2: home-office-atoms.ts (state core)

**Files:**
- Create: `ui/src/atoms/home-office-atoms.ts`
- Create: `ui/src/atoms/home-office-atoms.test.ts`
- Modify: `ui/src/atoms/index.ts` (if it exists — re-export pattern)

- [ ] **Step 1: Write failing test**

Create `ui/src/atoms/home-office-atoms.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  homeOfficePanelOpenAtom,
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  stickyNotesAtom,
  diaryEntriesAtom,
  openZoneAtom,
} from './home-office-atoms'

describe('home-office-atoms defaults', () => {
  it('panel starts closed', () => {
    const store = createStore()
    expect(store.get(homeOfficePanelOpenAtom)).toBe(false)
  })

  it('agent state defaults to idle', () => {
    const store = createStore()
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
  })

  it('character defaults near center (oak desk area)', () => {
    const store = createStore()
    const pos = store.get(characterPositionAtom)
    expect(pos.x).toBeGreaterThan(0.4)
    expect(pos.x).toBeLessThan(0.6)
    expect(pos.y).toBeGreaterThan(0.4)
    expect(pos.y).toBeLessThan(0.7)
  })

  it('character defaults facing south', () => {
    const store = createStore()
    expect(store.get(characterDirectionAtom)).toBe('S')
  })

  it('motion defaults to pose (not walking)', () => {
    const store = createStore()
    expect(store.get(characterMotionAtom)).toBe('pose')
  })

  it('sticky notes and diary entries start empty', () => {
    const store = createStore()
    expect(store.get(stickyNotesAtom)).toEqual([])
    expect(store.get(diaryEntriesAtom)).toEqual([])
  })

  it('no zone modal open by default', () => {
    const store = createStore()
    expect(store.get(openZoneAtom)).toBeNull()
  })
})

describe('home-office-atoms writes', () => {
  it('can set agent state to thinking', () => {
    const store = createStore()
    store.set(homeOfficeStateAtom, 'thinking')
    expect(store.get(homeOfficeStateAtom)).toBe('thinking')
  })

  it('can add sticky note', () => {
    const store = createStore()
    const note = { id: 'a', text: 'remember milk', at: 123 }
    store.set(stickyNotesAtom, [note])
    expect(store.get(stickyNotesAtom)).toEqual([note])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run home-office-atoms 2>&1 | tail -20`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement atoms**

Create `ui/src/atoms/home-office-atoms.ts`:

```typescript
/**
 * HomeOffice state atoms.
 *
 *  - Panel open/close (persisted via Settings later; runtime atom here)
 *  - Agent state machine (mirrors PetWidget's 5 states + tool_activity)
 *  - Character pose (position / direction / walk-vs-pose)
 *  - In-memory sticky notes + diary entries (Phase 4 persists them)
 *  - Currently-open zone modal
 */
import { atom } from 'jotai'

export const homeOfficePanelOpenAtom = atom(false)

export type HomeOfficeState =
  | 'idle'
  | 'thinking'
  | 'typing'
  | 'tool_activity'
  | 'success'
  | 'error'

export const homeOfficeStateAtom = atom<HomeOfficeState>('idle')

export type Vec2 = { x: number; y: number }

// Default position: in front of the central oak desk
export const characterPositionAtom = atom<Vec2>({ x: 0.50, y: 0.55 })

export type Direction = 'N' | 'NE' | 'E' | 'SE' | 'S' | 'SW' | 'W' | 'NW'
export const characterDirectionAtom = atom<Direction>('S')

export const characterMotionAtom = atom<'walk' | 'pose'>('pose')

export type StickyNote = { id: string; text: string; at: number }
export const stickyNotesAtom = atom<StickyNote[]>([])

export type DiaryEntry = { id: string; text: string; at: number; sessionId: string }
export const diaryEntriesAtom = atom<DiaryEntry[]>([])

export type OpenZone = null | 'music' | 'sticky' | 'diary'
export const openZoneAtom = atom<OpenZone>(null)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run home-office-atoms 2>&1 | tail -15`
Expected: PASS — 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/atoms/home-office-atoms.ts ui/src/atoms/home-office-atoms.test.ts
git commit -m "feat(home-office): atoms for state machine + character pose + modals"
```

---

## Task 3: dir-utils.ts (vec → 8-direction with mirror)

**Files:**
- Create: `ui/src/components/home-office/scene/dir-utils.ts`
- Create: `ui/src/components/home-office/scene/dir-utils.test.ts`

- [ ] **Step 1: Write failing test**

Create `ui/src/components/home-office/scene/dir-utils.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { vectorToDirection, resolveSpriteKey } from './dir-utils'

describe('vectorToDirection', () => {
  it('returns S for downward vector', () => {
    expect(vectorToDirection({ x: 0, y: 1 })).toBe('S')
  })

  it('returns N for upward vector', () => {
    expect(vectorToDirection({ x: 0, y: -1 })).toBe('N')
  })

  it('returns E for rightward vector', () => {
    expect(vectorToDirection({ x: 1, y: 0 })).toBe('E')
  })

  it('returns W for leftward vector', () => {
    expect(vectorToDirection({ x: -1, y: 0 })).toBe('W')
  })

  it('returns NE for up-right diagonal', () => {
    expect(vectorToDirection({ x: 0.7, y: -0.7 })).toBe('NE')
  })

  it('returns SW for down-left diagonal', () => {
    expect(vectorToDirection({ x: -0.7, y: 0.7 })).toBe('SW')
  })

  it('returns S for zero vector (default)', () => {
    expect(vectorToDirection({ x: 0, y: 0 })).toBe('S')
  })
})

describe('resolveSpriteKey (mirror optimization)', () => {
  it('uses walk-W for W', () => {
    expect(resolveSpriteKey('W')).toEqual({ key: 'walk-W', flipX: false })
  })

  it('mirrors W asset for E', () => {
    expect(resolveSpriteKey('E')).toEqual({ key: 'walk-W', flipX: true })
  })

  it('uses walk-NW for NW', () => {
    expect(resolveSpriteKey('NW')).toEqual({ key: 'walk-NW', flipX: false })
  })

  it('mirrors NW asset for NE', () => {
    expect(resolveSpriteKey('NE')).toEqual({ key: 'walk-NW', flipX: true })
  })

  it('mirrors SW asset for SE', () => {
    expect(resolveSpriteKey('SE')).toEqual({ key: 'walk-SW', flipX: true })
  })

  it('uses walk-N for N (no mirror)', () => {
    expect(resolveSpriteKey('N')).toEqual({ key: 'walk-N', flipX: false })
  })

  it('uses walk-S for S (no mirror)', () => {
    expect(resolveSpriteKey('S')).toEqual({ key: 'walk-S', flipX: false })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run dir-utils 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement utils**

Create `ui/src/components/home-office/scene/dir-utils.ts`:

```typescript
import type { Direction, Vec2 } from '@/atoms/home-office-atoms'

/**
 * Convert a 2D vector (e.g. target - current position) into the closest of 8
 * compass directions. Zero vector falls back to 'S' (default facing).
 */
export function vectorToDirection(v: Vec2): Direction {
  const { x, y } = v
  if (x === 0 && y === 0) return 'S'
  // atan2 with screen-coords: y grows downward, so 'S' is angle +90°.
  const angle = Math.atan2(y, x) // radians, -π..π
  const deg = (angle * 180) / Math.PI // -180..180
  // Snap to nearest 45° bucket
  const bucket = Math.round(deg / 45)
  switch (((bucket % 8) + 8) % 8) {
    case 0: return 'E'
    case 1: return 'SE'
    case 2: return 'S'
    case 3: return 'SW'
    case 4: return 'W'
    case 5: return 'NW'
    case 6: return 'N'
    case 7: return 'NE'
  }
  return 'S'
}

/**
 * Resolve a Direction to an asset key + horizontal flip flag.
 * Mirrors E/NE/SE off W/NW/SW so we only ship 5 walk WebPs per character.
 */
export function resolveSpriteKey(d: Direction): { key: string; flipX: boolean } {
  switch (d) {
    case 'E':  return { key: 'walk-W',  flipX: true }
    case 'NE': return { key: 'walk-NW', flipX: true }
    case 'SE': return { key: 'walk-SW', flipX: true }
    case 'W':  return { key: 'walk-W',  flipX: false }
    case 'NW': return { key: 'walk-NW', flipX: false }
    case 'SW': return { key: 'walk-SW', flipX: false }
    case 'N':  return { key: 'walk-N',  flipX: false }
    case 'S':  return { key: 'walk-S',  flipX: false }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run dir-utils 2>&1 | tail -15`
Expected: PASS — 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/home-office/scene/dir-utils.ts ui/src/components/home-office/scene/dir-utils.test.ts
git commit -m "feat(home-office): 8-direction vector mapping + sprite mirror resolver"
```

---

## Task 4: hit-areas.ts (zone constants)

**Files:**
- Create: `ui/src/components/home-office/scene/hit-areas.ts`

- [ ] **Step 1: Implement constants (no test — pure data)**

Create `ui/src/components/home-office/scene/hit-areas.ts`:

```typescript
/**
 * Zone hit-area coordinates, normalized to the 1920x1080 scene background.
 * `center` is the click target + character walk-to point; `w`/`h` are the
 * box used for pointer hit detection and hover highlight.
 *
 * `kind`:
 *   - 'modal': opens openZoneAtom = target
 *   - 'navigate': triggers a side effect (settings panel, history)
 *   - 'state': pure visual anchor; no click
 */
export type ZoneKind = 'modal' | 'navigate' | 'state'

export type Zone = {
  id: string
  center: { x: number; y: number }
  w: number
  h: number
  kind: ZoneKind
  target: 'music' | 'sticky' | 'diary' | 'skills' | 'history' | null
  label: string
}

export const ZONES: Record<string, Zone> = {
  garden:  { id: 'garden',  center: { x: 0.18, y: 0.78 }, w: 0.14, h: 0.18, kind: 'navigate', target: 'skills',  label: '🌿 Garden' },
  music:   { id: 'music',   center: { x: 0.13, y: 0.45 }, w: 0.18, h: 0.32, kind: 'modal',    target: 'music',   label: '🎵 Music Gazebo' },
  sticky:  { id: 'sticky',  center: { x: 0.36, y: 0.18 }, w: 0.16, h: 0.24, kind: 'modal',    target: 'sticky',  label: '📌 Sticky Wall' },
  diary:   { id: 'diary',   center: { x: 0.50, y: 0.45 }, w: 0.20, h: 0.40, kind: 'modal',    target: 'diary',   label: '✍️ Oak Desk' },
  library: { id: 'library', center: { x: 0.68, y: 0.22 }, w: 0.13, h: 0.34, kind: 'navigate', target: 'history', label: '📚 Library Tower' },
  fire:    { id: 'fire',    center: { x: 0.42, y: 0.75 }, w: 0.14, h: 0.18, kind: 'state',    target: null,      label: '🔥 Fire Pit' },
  hammock: { id: 'hammock', center: { x: 0.82, y: 0.62 }, w: 0.14, h: 0.18, kind: 'state',    target: null,      label: '🛋️ Hammock' },
  sakura:  { id: 'sakura',  center: { x: 0.70, y: 0.55 }, w: 0.14, h: 0.24, kind: 'state',    target: null,      label: '🌸 Sakura' },
} as const

// State → which zone the character should walk to
export const STATE_TO_ZONE: Record<string, keyof typeof ZONES | null> = {
  idle:          'hammock',
  thinking:      'library',
  typing:        'diary',
  tool_activity: 'fire',   // workshop/forge anchor — colocated with fire pit visually
  success:       null,     // stays in place 4s then walks to hammock
  error:         'fire',
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/hit-areas.ts
git commit -m "feat(home-office): zone hit-area constants + state→zone mapping"
```

---

## Task 5: useCharacterPath hook

**Files:**
- Create: `ui/src/hooks/useCharacterPath.ts`
- Create: `ui/src/hooks/useCharacterPath.test.ts`

This hook watches `homeOfficeStateAtom`, looks up the target zone, sets `characterMotionAtom='walk'`, lerps `characterPositionAtom` toward the target over time, and on arrival sets `characterMotionAtom='pose'`. Pure logic — testable with fake timers.

- [ ] **Step 1: Write failing test**

Create `ui/src/hooks/useCharacterPath.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import {
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
} from '@/atoms/home-office-atoms'
import { useCharacterPath } from './useCharacterPath'

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useCharacterPath', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('starts walking when state changes to thinking', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.5, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    expect(store.get(characterMotionAtom)).toBe('walk')
  })

  it('sets direction toward library tower for thinking', () => {
    const store = createStore()
    // start near oak desk
    store.set(characterPositionAtom, { x: 0.50, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    // library is at (0.68, 0.22) — up and to the right → NE
    expect(store.get(characterDirectionAtom)).toBe('NE')
  })

  it('reaches target and switches to pose', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.50, y: 0.55 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'thinking')
    })
    // Run ticker for plenty of time
    act(() => {
      vi.advanceTimersByTime(10_000)
    })
    expect(store.get(characterMotionAtom)).toBe('pose')
    const pos = store.get(characterPositionAtom)
    // close to library zone (0.68, 0.22)
    expect(Math.abs(pos.x - 0.68)).toBeLessThan(0.01)
    expect(Math.abs(pos.y - 0.22)).toBeLessThan(0.01)
  })

  it('success state stays in place (no walk)', () => {
    const store = createStore()
    store.set(characterPositionAtom, { x: 0.40, y: 0.60 })
    renderHook(() => useCharacterPath(), { wrapper: wrapper(store) })
    act(() => {
      store.set(homeOfficeStateAtom, 'success')
    })
    expect(store.get(characterMotionAtom)).toBe('pose')
    const pos = store.get(characterPositionAtom)
    expect(pos).toEqual({ x: 0.40, y: 0.60 })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run useCharacterPath 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement hook**

Create `ui/src/hooks/useCharacterPath.ts`:

```typescript
import { useEffect, useRef } from 'react'
import { useAtomValue, useSetAtom, useAtom } from 'jotai'
import {
  homeOfficeStateAtom,
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  type Vec2,
} from '@/atoms/home-office-atoms'
import { ZONES, STATE_TO_ZONE } from '@/components/home-office/scene/hit-areas'
import { vectorToDirection } from '@/components/home-office/scene/dir-utils'

// Normalized scene units per millisecond. 0.4 units in ~2.5s feels natural.
const WALK_SPEED = 0.4 / 2500
const ARRIVE_EPSILON = 0.005
const TICK_MS = 33 // ~30 Hz logic tick (sprite anim runs separately)

export function useCharacterPath() {
  const state = useAtomValue(homeOfficeStateAtom)
  const [position, setPosition] = useAtom(characterPositionAtom)
  const setDirection = useSetAtom(characterDirectionAtom)
  const setMotion = useSetAtom(characterMotionAtom)

  const targetRef = useRef<Vec2 | null>(null)
  const positionRef = useRef(position)
  positionRef.current = position

  // On state change → choose target zone (or null = stay put)
  useEffect(() => {
    const zoneKey = STATE_TO_ZONE[state]
    if (!zoneKey) {
      targetRef.current = null
      setMotion('pose')
      return
    }
    const zone = ZONES[zoneKey]
    targetRef.current = zone.center
    const dx = zone.center.x - positionRef.current.x
    const dy = zone.center.y - positionRef.current.y
    if (Math.hypot(dx, dy) < ARRIVE_EPSILON) {
      setMotion('pose')
      return
    }
    setDirection(vectorToDirection({ x: dx, y: dy }))
    setMotion('walk')
  }, [state, setDirection, setMotion])

  // Lerp tick — fires while a target is set, stops on arrival.
  useEffect(() => {
    const id = setInterval(() => {
      const target = targetRef.current
      if (!target) return
      const cur = positionRef.current
      const dx = target.x - cur.x
      const dy = target.y - cur.y
      const dist = Math.hypot(dx, dy)
      if (dist < ARRIVE_EPSILON) {
        setPosition({ x: target.x, y: target.y })
        setMotion('pose')
        targetRef.current = null
        return
      }
      const step = WALK_SPEED * TICK_MS
      const ratio = Math.min(step / dist, 1)
      setPosition({ x: cur.x + dx * ratio, y: cur.y + dy * ratio })
    }, TICK_MS)
    return () => clearInterval(id)
  }, [setPosition, setMotion])
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run useCharacterPath 2>&1 | tail -15`
Expected: PASS — 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/hooks/useCharacterPath.ts ui/src/hooks/useCharacterPath.test.ts
git commit -m "feat(home-office): useCharacterPath — state→zone walk + direction selection"
```

---

## Task 6: useHomeOfficeAgentSync hook

**Files:**
- Create: `ui/src/hooks/useHomeOfficeAgentSync.ts`
- Create: `ui/src/hooks/useHomeOfficeAgentSync.test.ts`

Wires the 5 Tauri events to `homeOfficeStateAtom`. Mirrors `usePetStateSync` but with HomeOffice state enum and `tool_activity` as its own state value.

- [ ] **Step 1: Write failing test**

Create `ui/src/hooks/useHomeOfficeAgentSync.test.ts`:

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { homeOfficeStateAtom } from '@/atoms/home-office-atoms'
import { useHomeOfficeAgentSync } from './useHomeOfficeAgentSync'

type Listener = (event: { payload: unknown }) => void
const listeners = new Map<string, Listener>()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (channel: string, handler: Listener) => {
    listeners.set(channel, handler)
    return () => { listeners.delete(channel) }
  }),
}))

function fire(channel: string, payload: unknown = {}) {
  const h = listeners.get(channel)
  if (h) h({ payload })
}

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

describe('useHomeOfficeAgentSync', () => {
  beforeEach(() => {
    listeners.clear()
    vi.clearAllMocks()
  })

  it('maps chat:stream-chunk → typing', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-chunk'))
    expect(store.get(homeOfficeStateAtom)).toBe('typing')
  })

  it('maps chat:stream-tool-activity → tool_activity', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-tool-activity'))
    expect(store.get(homeOfficeStateAtom)).toBe('tool_activity')
  })

  it('maps chat:stream-error → error', async () => {
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-error'))
    expect(store.get(homeOfficeStateAtom)).toBe('error')
  })

  it('chat:stream-complete → success then idle after 4s', async () => {
    vi.useFakeTimers()
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-complete'))
    expect(store.get(homeOfficeStateAtom)).toBe('success')
    act(() => { vi.advanceTimersByTime(4000) })
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    vi.useRealTimers()
  })

  it('agent:stream-reset → idle (cancels pending success timer)', async () => {
    vi.useFakeTimers()
    const store = createStore()
    renderHook(() => useHomeOfficeAgentSync(), { wrapper: wrapper(store) })
    await act(async () => { await Promise.resolve() })
    act(() => fire('chat:stream-complete'))
    expect(store.get(homeOfficeStateAtom)).toBe('success')
    act(() => fire('agent:stream-reset'))
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    // even if the 4s success timer fires later, state should not flip
    act(() => { vi.advanceTimersByTime(5000) })
    expect(store.get(homeOfficeStateAtom)).toBe('idle')
    vi.useRealTimers()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run useHomeOfficeAgentSync 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement hook**

Create `ui/src/hooks/useHomeOfficeAgentSync.ts`:

```typescript
import { useEffect } from 'react'
import { useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { homeOfficeStateAtom } from '@/atoms/home-office-atoms'

const SUCCESS_LINGER_MS = 4000

export function useHomeOfficeAgentSync() {
  const setState = useSetAtom(homeOfficeStateAtom)

  useEffect(() => {
    const unlisten: Array<() => void> = []
    let successTimer: ReturnType<typeof setTimeout> | null = null

    const clearSuccessTimer = () => {
      if (successTimer) {
        clearTimeout(successTimer)
        successTimer = null
      }
    }

    listen('chat:stream-chunk', () => {
      clearSuccessTimer()
      setState('typing')
    }).then(u => unlisten.push(u))

    listen('chat:stream-tool-activity', () => {
      clearSuccessTimer()
      setState('tool_activity')
    }).then(u => unlisten.push(u))

    listen('chat:stream-complete', () => {
      clearSuccessTimer()
      setState('success')
      successTimer = setTimeout(() => {
        setState('idle')
        successTimer = null
      }, SUCCESS_LINGER_MS)
    }).then(u => unlisten.push(u))

    listen('chat:stream-error', () => {
      clearSuccessTimer()
      setState('error')
    }).then(u => unlisten.push(u))

    listen('agent:stream-reset', () => {
      clearSuccessTimer()
      setState('idle')
    }).then(u => unlisten.push(u))

    return () => {
      clearSuccessTimer()
      unlisten.forEach(u => u())
    }
  }, [setState])
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run useHomeOfficeAgentSync 2>&1 | tail -15`
Expected: PASS — 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/hooks/useHomeOfficeAgentSync.ts ui/src/hooks/useHomeOfficeAgentSync.test.ts
git commit -m "feat(home-office): Tauri agent IPC → state atom (with success linger + reset cancel)"
```

---

## Task 7: sprite-loader.ts + animator.ts

**Files:**
- Create: `ui/src/components/home-office/scene/sprite-loader.ts`
- Create: `ui/src/components/home-office/scene/animator.ts`
- Create: `ui/src/components/home-office/scene/animator.test.ts`

Sprite loader uses `ImageDecoder` to decode animated WebP into frames. Animator is a pure-JS class that swaps a `PIXI.Sprite`'s `.texture` based on elapsed ms. Animator gets full tests; loader gets a smoke test only (jsdom lacks `ImageDecoder` and WebGL).

- [ ] **Step 1: Write failing animator test**

Create `ui/src/components/home-office/scene/animator.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { WebpAnimator } from './animator'

// Lightweight Sprite stub with a settable .texture
class FakeSprite { texture: unknown = null }
function fakeTextures(n: number) {
  return Array.from({ length: n }, (_, i) => ({ id: `tex-${i}` }) as unknown)
}

describe('WebpAnimator', () => {
  it('starts on frame 0', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    expect(sprite.texture).toBe(frames[0])
  })

  it('advances one frame per ~42ms at 24fps', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(42) // 1000/24 ≈ 41.67
    expect(sprite.texture).toBe(frames[1])
    a.tick(42)
    expect(sprite.texture).toBe(frames[2])
  })

  it('loops back to frame 0', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(3)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(42 * 3) // ~3 frame ticks
    expect(sprite.texture).toBe(frames[0])
  })

  it('swap() resets to frame 0 of new sequence', () => {
    const sprite = new FakeSprite()
    const seqA = fakeTextures(4)
    const seqB = fakeTextures(2)
    const a = new WebpAnimator(sprite as never, seqA as never, 24)
    a.tick(42 * 2)
    a.swap(seqB as never)
    expect(sprite.texture).toBe(seqB[0])
    a.tick(42)
    expect(sprite.texture).toBe(seqB[1])
  })

  it('accumulates sub-frame deltas correctly', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(20)
    expect(sprite.texture).toBe(frames[0])
    a.tick(25) // 20+25=45 → cross threshold
    expect(sprite.texture).toBe(frames[1])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run animator 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement animator**

Create `ui/src/components/home-office/scene/animator.ts`:

```typescript
import type { Sprite, Texture } from 'pixi.js'

/**
 * Drives a PIXI.Sprite's .texture through a frame sequence at a fixed fps.
 * Caller invokes tick(deltaMs) on every render frame (PIXI.Ticker).
 */
export class WebpAnimator {
  private currentFrame = 0
  private accumulatedMs = 0
  private readonly frameDurationMs: number

  constructor(
    private readonly sprite: Sprite,
    private frames: Texture[],
    fps = 24,
  ) {
    this.frameDurationMs = 1000 / fps
    if (frames.length > 0) this.sprite.texture = frames[0]
  }

  tick(deltaMs: number) {
    if (this.frames.length === 0) return
    this.accumulatedMs += deltaMs
    while (this.accumulatedMs >= this.frameDurationMs) {
      this.accumulatedMs -= this.frameDurationMs
      this.currentFrame = (this.currentFrame + 1) % this.frames.length
      this.sprite.texture = this.frames[this.currentFrame]
    }
  }

  swap(newFrames: Texture[]) {
    this.frames = newFrames
    this.currentFrame = 0
    this.accumulatedMs = 0
    if (newFrames.length > 0) this.sprite.texture = newFrames[0]
  }
}
```

- [ ] **Step 4: Run test to verify animator passes**

Run: `cd ui && npm test -- --run animator 2>&1 | tail -15`
Expected: PASS — 5 tests pass.

- [ ] **Step 5: Implement sprite-loader**

Create `ui/src/components/home-office/scene/sprite-loader.ts`:

```typescript
import { Assets, Texture } from 'pixi.js'

/**
 * Decode an animated WebP into an array of PIXI.Textures via the browser's
 * ImageDecoder API. Tauri webview (WebKit on macOS, WebView2 on Win,
 * WebKitGTK on Linux) supports ImageDecoder as of mid-2024.
 *
 * Fallback (no ImageDecoder available, e.g. jsdom): returns a single static
 * texture so callers degrade gracefully.
 */
export async function loadAnimatedWebp(url: string): Promise<Texture[]> {
  if (typeof window === 'undefined' || !('ImageDecoder' in window)) {
    const tex = await Assets.load<Texture>(url)
    return [tex]
  }

  const res = await fetch(url)
  if (!res.ok || !res.body) {
    const tex = await Assets.load<Texture>(url)
    return [tex]
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Decoder = (window as any).ImageDecoder
  const decoder = new Decoder({ data: res.body, type: 'image/webp' })
  await decoder.completed

  const track = decoder.tracks.selectedTrack
  const frameCount: number = track?.frameCount ?? 1
  const textures: Texture[] = []
  for (let i = 0; i < frameCount; i++) {
    const { image } = await decoder.decode({ frameIndex: i })
    const bitmap = await createImageBitmap(image)
    textures.push(Texture.from(bitmap))
  }
  return textures
}

const cache = new Map<string, Promise<Texture[]>>()

export function loadAnimatedWebpCached(url: string): Promise<Texture[]> {
  let p = cache.get(url)
  if (!p) {
    p = loadAnimatedWebp(url)
    cache.set(url, p)
  }
  return p
}

export function clearSpriteCache() {
  cache.clear()
}
```

- [ ] **Step 6: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/home-office/scene/sprite-loader.ts \
        ui/src/components/home-office/scene/animator.ts \
        ui/src/components/home-office/scene/animator.test.ts
git commit -m "feat(home-office): WebpAnimator + ImageDecoder-based sprite loader"
```

---

## Task 8: BackgroundLayer

**Files:**
- Create: `ui/src/components/home-office/scene/layers/BackgroundLayer.tsx`

Single static sprite of `scene-sky-v5.png` stretched to scene size. No tests — pure pixi-react JSX with no logic.

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/scene/layers/BackgroundLayer.tsx`:

```typescript
import { useEffect, useState } from 'react'
import { Assets, Texture } from 'pixi.js'

type Props = { width: number; height: number }

export function BackgroundLayer({ width, height }: Props) {
  const [texture, setTexture] = useState<Texture | null>(null)

  useEffect(() => {
    let cancelled = false
    Assets.load<Texture>('/home-office/scene-sky-v5.png').then(t => {
      if (!cancelled) setTexture(t)
    })
    return () => { cancelled = true }
  }, [])

  if (!texture) return null
  return (
    <pixiSprite
      texture={texture}
      x={0}
      y={0}
      width={width}
      height={height}
    />
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors. (If `pixiSprite` lowercase intrinsic is unknown, ensure `@pixi/react` v8 is correctly installed — v8 uses lowercase intrinsics.)

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/layers/BackgroundLayer.tsx
git commit -m "feat(home-office): BackgroundLayer renders v5 scene"
```

---

## Task 9: ZoneLayer

**Files:**
- Create: `ui/src/components/home-office/scene/layers/ZoneLayer.tsx`

Renders 8 zone hit-areas as invisible interactive rectangles with hover highlight + click router.

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/scene/layers/ZoneLayer.tsx`:

```typescript
import { useCallback, useState } from 'react'
import { useSetAtom } from 'jotai'
import { Graphics } from 'pixi.js'
import { openZoneAtom, homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { ZONES, type Zone } from '../hit-areas'

type Props = { width: number; height: number }

export function ZoneLayer({ width, height }: Props) {
  const setOpenZone = useSetAtom(openZoneAtom)
  const setPanelOpen = useSetAtom(homeOfficePanelOpenAtom)
  const [hover, setHover] = useState<string | null>(null)

  const onClick = useCallback((zone: Zone) => {
    if (zone.kind === 'modal' && zone.target) {
      // 'music' | 'sticky' | 'diary' — narrowed by Zone type
      setOpenZone(zone.target as 'music' | 'sticky' | 'diary')
    } else if (zone.kind === 'navigate') {
      // Leave HomeOffice and navigate to the requested panel.
      // (Skills / history are existing routes; the consumer handles them.)
      setPanelOpen(false)
      if (zone.target === 'skills') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'skills' }))
      } else if (zone.target === 'history') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'history' }))
      }
    }
  }, [setOpenZone, setPanelOpen])

  return (
    <pixiContainer>
      {Object.values(ZONES).map(zone => {
        const x = (zone.center.x - zone.w / 2) * width
        const y = (zone.center.y - zone.h / 2) * height
        const w = zone.w * width
        const h = zone.h * height
        const isHover = hover === zone.id
        const interactive = zone.kind !== 'state'
        return (
          <pixiGraphics
            key={zone.id}
            eventMode={interactive ? 'static' : 'none'}
            cursor={interactive ? 'pointer' : 'default'}
            x={x}
            y={y}
            onPointerOver={() => interactive && setHover(zone.id)}
            onPointerOut={() => setHover(prev => (prev === zone.id ? null : prev))}
            onPointerTap={() => interactive && onClick(zone)}
            draw={(g: Graphics) => {
              g.clear()
              if (isHover) {
                g.setStrokeStyle({ width: 3, color: 0xffd97a, alpha: 0.9 })
                g.rect(0, 0, w, h)
                g.stroke()
                g.fill({ color: 0xffd97a, alpha: 0.08 })
                g.rect(0, 0, w, h)
                g.fill()
              } else {
                // Invisible hit area
                g.rect(0, 0, w, h)
                g.fill({ color: 0xffffff, alpha: 0 })
              }
            }}
          />
        )
      })}
    </pixiContainer>
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/layers/ZoneLayer.tsx
git commit -m "feat(home-office): ZoneLayer with 8 hit-areas + hover highlight + click router"
```

---

## Task 10: CharacterLayer

**Files:**
- Create: `ui/src/components/home-office/scene/layers/CharacterLayer.tsx`

Resolves which sprite to show (walk-X or pose-X), loads its frames via `loadAnimatedWebpCached`, drives a `WebpAnimator` via PIXI ticker, and positions the sprite from `characterPositionAtom`.

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/scene/layers/CharacterLayer.tsx`:

```typescript
import { useEffect, useRef, useState } from 'react'
import { useAtomValue } from 'jotai'
import { useTick } from '@pixi/react'
import type { Sprite, Texture } from 'pixi.js'
import {
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  homeOfficeStateAtom,
} from '@/atoms/home-office-atoms'
import { resolveSpriteKey } from '../dir-utils'
import { loadAnimatedWebpCached } from '../sprite-loader'
import { WebpAnimator } from '../animator'

type Props = { width: number; height: number }

const SPRITE_BASE = '/home-office/sprites/lofi-girl'
const SPRITE_W = 160
const SPRITE_H = 160

function spriteUrlForMotion(
  motion: 'walk' | 'pose',
  direction: string,
  state: string,
): { url: string; flipX: boolean } {
  if (motion === 'walk') {
    const { key, flipX } = resolveSpriteKey(direction as never)
    return { url: `${SPRITE_BASE}/${key}.webp`, flipX }
  }
  // Pose maps off state. 'tool_activity' visually shares 'thinking' pose.
  const poseState = state === 'tool_activity' ? 'thinking' : state
  return { url: `${SPRITE_BASE}/pose-${poseState}.webp`, flipX: false }
}

export function CharacterLayer({ width, height }: Props) {
  const pos = useAtomValue(characterPositionAtom)
  const direction = useAtomValue(characterDirectionAtom)
  const motion = useAtomValue(characterMotionAtom)
  const state = useAtomValue(homeOfficeStateAtom)

  const spriteRef = useRef<Sprite | null>(null)
  const animatorRef = useRef<WebpAnimator | null>(null)
  const [currentFrames, setCurrentFrames] = useState<Texture[]>([])
  const [flipX, setFlipX] = useState(false)

  // Resolve + load frames when motion/direction/state changes
  useEffect(() => {
    const { url, flipX: needsFlip } = spriteUrlForMotion(motion, direction, state)
    setFlipX(needsFlip)
    let cancelled = false
    loadAnimatedWebpCached(url).then(frames => {
      if (cancelled) return
      setCurrentFrames(frames)
      if (animatorRef.current) {
        animatorRef.current.swap(frames)
      } else if (spriteRef.current) {
        animatorRef.current = new WebpAnimator(spriteRef.current, frames, 24)
      }
    }).catch(() => {
      // Loader handles fallback; if it still rejects, silently skip.
    })
    return () => { cancelled = true }
  }, [motion, direction, state])

  useTick(({ deltaMS }) => {
    animatorRef.current?.tick(deltaMS)
  })

  const screenX = pos.x * width
  const screenY = pos.y * height

  return (
    <pixiSprite
      ref={spriteRef}
      texture={currentFrames[0]}
      x={screenX}
      y={screenY}
      width={SPRITE_W}
      height={SPRITE_H}
      anchor={0.5}
      scale={{ x: flipX ? -1 : 1, y: 1 }}
    />
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/layers/CharacterLayer.tsx
git commit -m "feat(home-office): CharacterLayer wires sprite to atoms + WebpAnimator ticker"
```

---

## Task 11: ParticleLayer

**Files:**
- Create: `ui/src/components/home-office/scene/layers/ParticleLayer.tsx`

Draws sakura petals (drifting down-left) + kodama (Lissajous floats) + waterfall mist (rising puffs). Pure PIXI Graphics, no assets.

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/scene/layers/ParticleLayer.tsx`:

```typescript
import { useRef } from 'react'
import { useTick } from '@pixi/react'
import type { Graphics } from 'pixi.js'

type Props = { width: number; height: number }

type Petal = { x: number; y: number; vx: number; vy: number; rot: number; r: number }
type Kodama = { cx: number; cy: number; rx: number; ry: number; phase: number; speed: number }
type Mist = { x: number; y: number; life: number; maxLife: number }

const PETAL_COUNT = 8
const KODAMA_COUNT = 3
const MIST_COUNT = 10

function spawnPetal(width: number, height: number): Petal {
  return {
    x: width * 0.5 + Math.random() * width * 0.5,
    y: -10,
    vx: -0.06 - Math.random() * 0.04,
    vy: 0.04 + Math.random() * 0.03,
    rot: Math.random() * Math.PI * 2,
    r: 4 + Math.random() * 3,
  }
}

function spawnMist(width: number, height: number): Mist {
  const onLeft = Math.random() < 0.5
  return {
    x: onLeft ? width * 0.30 : width * 0.78,
    y: height * 0.82,
    life: 0,
    maxLife: 800 + Math.random() * 400,
  }
}

export function ParticleLayer({ width, height }: Props) {
  const petalsRef = useRef<Petal[]>(Array.from({ length: PETAL_COUNT }, () => spawnPetal(width, height)))
  const kodamaRef = useRef<Kodama[]>(Array.from({ length: KODAMA_COUNT }, (_, i) => ({
    cx: width * (0.30 + i * 0.18),
    cy: height * (0.55 + (i % 2) * 0.08),
    rx: 40 + Math.random() * 20,
    ry: 20 + Math.random() * 15,
    phase: Math.random() * Math.PI * 2,
    speed: 0.0006 + Math.random() * 0.0004,
  })))
  const mistRef = useRef<Mist[]>(Array.from({ length: MIST_COUNT }, () => spawnMist(width, height)))
  const gRef = useRef<Graphics | null>(null)

  useTick(({ deltaMS }) => {
    const g = gRef.current
    if (!g) return

    // Update petals
    for (const p of petalsRef.current) {
      p.x += p.vx * deltaMS
      p.y += p.vy * deltaMS
      p.rot += 0.002 * deltaMS
      if (p.y > height + 10 || p.x < -10) {
        Object.assign(p, spawnPetal(width, height))
      }
    }

    // Update kodama
    for (const k of kodamaRef.current) {
      k.phase += k.speed * deltaMS
    }

    // Update mist
    for (const m of mistRef.current) {
      m.life += deltaMS
      if (m.life > m.maxLife) Object.assign(m, spawnMist(width, height))
    }

    // Redraw
    g.clear()

    for (const p of petalsRef.current) {
      g.fill({ color: 0xffc1d1, alpha: 0.8 })
      g.ellipse(p.x, p.y, p.r, p.r * 0.55)
      g.fill()
    }

    for (const k of kodamaRef.current) {
      const kx = k.cx + Math.cos(k.phase) * k.rx
      const ky = k.cy + Math.sin(k.phase * 1.3) * k.ry
      g.fill({ color: 0xffffff, alpha: 0.9 })
      g.circle(kx, ky, 6)
      g.fill()
      g.fill({ color: 0x222222, alpha: 1 })
      g.circle(kx - 2, ky - 1, 0.9)
      g.circle(kx + 2, ky - 1, 0.9)
      g.fill()
    }

    for (const m of mistRef.current) {
      const t = m.life / m.maxLife
      const yOff = -t * 40
      const alpha = (1 - t) * 0.35
      g.fill({ color: 0xffffff, alpha })
      g.circle(m.x + Math.sin(t * 3) * 4, m.y + yOff, 6 + t * 5)
      g.fill()
    }
  })

  return <pixiGraphics ref={gRef} />
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/layers/ParticleLayer.tsx
git commit -m "feat(home-office): ParticleLayer — sakura + kodama + waterfall mist"
```

---

## Task 12: HomeOfficeScene composition

**Files:**
- Create: `ui/src/components/home-office/scene/HomeOfficeScene.tsx`

Assembles all 4 layers inside a pixi-react `Application`. Resize observer keeps width/height in sync with the container.

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/scene/HomeOfficeScene.tsx`:

```typescript
import { useEffect, useRef, useState } from 'react'
import { Application, extend } from '@pixi/react'
import { Container, Graphics, Sprite } from 'pixi.js'
import { BackgroundLayer } from './layers/BackgroundLayer'
import { ZoneLayer } from './layers/ZoneLayer'
import { CharacterLayer } from './layers/CharacterLayer'
import { ParticleLayer } from './layers/ParticleLayer'

// v8 of @pixi/react requires registering pixi classes used in JSX
extend({ Container, Graphics, Sprite })

export function HomeOfficeScene() {
  const wrapRef = useRef<HTMLDivElement | null>(null)
  const [size, setSize] = useState({ w: 1280, h: 720 })

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => {
      for (const e of entries) {
        const { width, height } = e.contentRect
        if (width > 0 && height > 0) setSize({ w: width, h: height })
      }
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  return (
    <div ref={wrapRef} className="relative w-full h-full overflow-hidden bg-content-area">
      <Application
        width={size.w}
        height={size.h}
        background={0x88ccee}
        antialias
        resolution={window.devicePixelRatio || 1}
        autoDensity
      >
        <BackgroundLayer width={size.w} height={size.h} />
        <ZoneLayer width={size.w} height={size.h} />
        <CharacterLayer width={size.w} height={size.h} />
        <ParticleLayer width={size.w} height={size.h} />
      </Application>
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/scene/HomeOfficeScene.tsx
git commit -m "feat(home-office): HomeOfficeScene — Application + 4 layers + ResizeObserver"
```

---

## Task 13: MusicGazeboModal (audio player)

**Files:**
- Create: `ui/src/components/home-office/zones/MusicGazeboModal.tsx`
- Create: `ui/public/home-office/audio/lofi-placeholder.mp3` (any tiny CC0 or silent track)

- [ ] **Step 1: Add a silent placeholder audio file**

```bash
cd ui/public/home-office/audio
# Generate a 3-second silent MP3 via ffmpeg (available in dev env)
ffmpeg -f lavfi -i anullsrc=channel_layout=stereo:sample_rate=44100 -t 3 -q:a 9 -acodec libmp3lame lofi-placeholder.mp3 -y 2>&1 | tail -3
ls -la lofi-placeholder.mp3
```

Expected: ~5KB MP3 file exists.

- [ ] **Step 2: Implement modal**

Create `ui/src/components/home-office/zones/MusicGazeboModal.tsx`:

```typescript
import { useEffect, useRef, useState } from 'react'
import { useAtom } from 'jotai'
import { openZoneAtom } from '@/atoms/home-office-atoms'

const TRACKS = [
  { id: 'placeholder-1', title: 'Lofi Placeholder', src: '/home-office/audio/lofi-placeholder.mp3' },
]

export function MusicGazeboModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const audioRef = useRef<HTMLAudioElement | null>(null)
  const [playing, setPlaying] = useState(false)
  const [trackIndex, setTrackIndex] = useState(0)

  useEffect(() => {
    if (openZone !== 'music') {
      audioRef.current?.pause()
      setPlaying(false)
    }
  }, [openZone])

  if (openZone !== 'music') return null

  const track = TRACKS[trackIndex]
  const togglePlay = () => {
    const a = audioRef.current
    if (!a) return
    if (playing) { a.pause(); setPlaying(false) }
    else { a.play().then(() => setPlaying(true)).catch(() => setPlaying(false)) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[360px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-base font-semibold">🎵 Music Gazebo</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <div className="text-sm mb-3">
          <div className="font-medium">{track.title}</div>
          <div className="text-muted-foreground text-xs">Track {trackIndex + 1} / {TRACKS.length}</div>
        </div>
        <audio
          ref={audioRef}
          src={track.src}
          onEnded={() => setPlaying(false)}
        />
        <div className="flex gap-2">
          <button
            onClick={togglePlay}
            className="px-3 py-1.5 bg-accent text-accent-foreground rounded-md text-sm hover:bg-accent/80"
          >
            {playing ? 'Pause' : 'Play'}
          </button>
          <button
            onClick={() => setTrackIndex(i => (i + 1) % TRACKS.length)}
            disabled={TRACKS.length <= 1}
            className="px-3 py-1.5 bg-secondary text-secondary-foreground rounded-md text-sm hover:bg-secondary/80 disabled:opacity-40"
          >
            Next
          </button>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/home-office/zones/MusicGazeboModal.tsx ui/public/home-office/audio/lofi-placeholder.mp3
git commit -m "feat(home-office): MusicGazeboModal with HTML5 audio + placeholder track"
```

---

## Task 14: StickyNoteModal (in-memory CRUD)

**Files:**
- Create: `ui/src/components/home-office/zones/StickyNoteModal.tsx`

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/zones/StickyNoteModal.tsx`:

```typescript
import { useState } from 'react'
import { useAtom } from 'jotai'
import { openZoneAtom, stickyNotesAtom, type StickyNote } from '@/atoms/home-office-atoms'

function newId() {
  return Math.random().toString(36).slice(2, 10)
}

export function StickyNoteModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const [notes, setNotes] = useAtom(stickyNotesAtom)
  const [draft, setDraft] = useState('')

  if (openZone !== 'sticky') return null

  const add = () => {
    if (!draft.trim()) return
    const note: StickyNote = { id: newId(), text: draft.trim(), at: Date.now() }
    setNotes([note, ...notes])
    setDraft('')
  }

  const remove = (id: string) => setNotes(notes.filter(n => n.id !== id))

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[440px] max-w-[560px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-base font-semibold">📌 Sticky Notes</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <p className="text-xs text-muted-foreground mb-3">暂存，重启丢失（Phase 4 持久化）</p>

        <div className="flex gap-2 mb-4">
          <input
            value={draft}
            onChange={e => setDraft(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && add()}
            placeholder="写一条便签…"
            className="flex-1 px-3 py-1.5 rounded-md bg-input text-foreground border border-border text-sm"
          />
          <button
            onClick={add}
            disabled={!draft.trim()}
            className="px-3 py-1.5 bg-accent text-accent-foreground rounded-md text-sm hover:bg-accent/80 disabled:opacity-40"
          >
            Add
          </button>
        </div>

        <div className="max-h-[300px] overflow-y-auto space-y-2">
          {notes.length === 0 && (
            <div className="text-sm text-muted-foreground italic">还没有便签</div>
          )}
          {notes.map(note => (
            <div key={note.id}
                 className="flex items-start justify-between gap-2 p-2 bg-secondary/40 rounded-md">
              <div className="text-sm flex-1">{note.text}</div>
              <button
                onClick={() => remove(note.id)}
                className="text-muted-foreground hover:text-foreground text-xs"
                title="删除"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/zones/StickyNoteModal.tsx
git commit -m "feat(home-office): StickyNoteModal — in-memory CRUD with theme tokens"
```

---

## Task 15: DiaryDeskModal (read-only)

**Files:**
- Create: `ui/src/components/home-office/zones/DiaryDeskModal.tsx`

- [ ] **Step 1: Implement**

Create `ui/src/components/home-office/zones/DiaryDeskModal.tsx`:

```typescript
import { useAtom, useAtomValue } from 'jotai'
import { openZoneAtom, diaryEntriesAtom } from '@/atoms/home-office-atoms'

export function DiaryDeskModal() {
  const [openZone, setOpenZone] = useAtom(openZoneAtom)
  const entries = useAtomValue(diaryEntriesAtom)

  if (openZone !== 'diary') return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
         onClick={() => setOpenZone(null)}>
      <div className="bg-popover text-popover-foreground rounded-xl shadow-2xl p-6 min-w-[440px] max-w-[560px]"
           onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-base font-semibold">✍️ Agent Diary</h3>
          <button onClick={() => setOpenZone(null)}
                  className="text-muted-foreground hover:text-foreground text-lg leading-none">×</button>
        </div>
        <p className="text-xs text-muted-foreground mb-3">暂存，重启丢失（Phase 4 持久化 + 按 session 归档）</p>
        <div className="max-h-[360px] overflow-y-auto space-y-3">
          {entries.length === 0 && (
            <div className="text-sm text-muted-foreground italic">Agent 还没有写过日记</div>
          )}
          {entries.map(entry => (
            <div key={entry.id} className="p-3 bg-secondary/40 rounded-md">
              <div className="text-xs text-muted-foreground mb-1">
                {new Date(entry.at).toLocaleString()} · session {entry.sessionId.slice(0, 8)}
              </div>
              <div className="text-sm whitespace-pre-wrap">{entry.text}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: TypeScript check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/home-office/zones/DiaryDeskModal.tsx
git commit -m "feat(home-office): DiaryDeskModal — read-only agent journal display"
```

---

## Task 16: HomeOfficeView page container + test

**Files:**
- Create: `ui/src/components/home-office/HomeOfficeView.tsx`
- Create: `ui/src/components/home-office/HomeOfficeView.test.tsx`

Mounts scene + all 3 modals + activates state-sync hooks.

- [ ] **Step 1: Write failing test**

Create `ui/src/components/home-office/HomeOfficeView.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { openZoneAtom, stickyNotesAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeView } from './HomeOfficeView'

// Mock the PIXI scene — jsdom can't render WebGL
vi.mock('./scene/HomeOfficeScene', () => ({
  HomeOfficeScene: () => <div data-testid="pixi-stage" />,
}))

// Mock Tauri event listener
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <HomeOfficeView />
    </Provider>,
  )
}

describe('HomeOfficeView', () => {
  it('renders the pixi scene stub', () => {
    const store = createStore()
    renderWith(store)
    expect(screen.getByTestId('pixi-stage')).toBeInTheDocument()
  })

  it('shows StickyNoteModal when openZone=sticky', () => {
    const store = createStore()
    store.set(openZoneAtom, 'sticky')
    renderWith(store)
    expect(screen.getByText('📌 Sticky Notes')).toBeInTheDocument()
  })

  it('can add a sticky note via the modal', () => {
    const store = createStore()
    store.set(openZoneAtom, 'sticky')
    renderWith(store)
    const input = screen.getByPlaceholderText('写一条便签…')
    fireEvent.change(input, { target: { value: 'hello' } })
    fireEvent.click(screen.getByText('Add'))
    expect(store.get(stickyNotesAtom)).toHaveLength(1)
    expect(store.get(stickyNotesAtom)[0].text).toBe('hello')
  })

  it('shows DiaryDeskModal when openZone=diary', () => {
    const store = createStore()
    store.set(openZoneAtom, 'diary')
    renderWith(store)
    expect(screen.getByText('✍️ Agent Diary')).toBeInTheDocument()
  })

  it('shows MusicGazeboModal when openZone=music', () => {
    const store = createStore()
    store.set(openZoneAtom, 'music')
    renderWith(store)
    expect(screen.getByText('🎵 Music Gazebo')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run HomeOfficeView 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement view**

Create `ui/src/components/home-office/HomeOfficeView.tsx`:

```typescript
import { useSetAtom } from 'jotai'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeScene } from './scene/HomeOfficeScene'
import { MusicGazeboModal } from './zones/MusicGazeboModal'
import { StickyNoteModal } from './zones/StickyNoteModal'
import { DiaryDeskModal } from './zones/DiaryDeskModal'
import { useHomeOfficeAgentSync } from '@/hooks/useHomeOfficeAgentSync'
import { useCharacterPath } from '@/hooks/useCharacterPath'

export function HomeOfficeView() {
  const setOpen = useSetAtom(homeOfficePanelOpenAtom)
  useHomeOfficeAgentSync()
  useCharacterPath()

  return (
    <div className="flex flex-col w-full h-full">
      <div className="flex items-center justify-between px-4 h-[34px] flex-shrink-0 border-b border-border/40 titlebar-no-drag">
        <span className="text-[13px] font-semibold flex items-center gap-1.5">
          <span>🏝️ Home Office</span>
        </span>
        <button
          onClick={() => setOpen(false)}
          className="text-muted-foreground hover:text-foreground text-[18px] leading-none w-6 h-6 flex items-center justify-center rounded-md hover:bg-accent"
          title="返回 (Esc)"
        >
          ×
        </button>
      </div>
      <div className="flex-1 min-h-0 relative titlebar-no-drag">
        <HomeOfficeScene />
        <MusicGazeboModal />
        <StickyNoteModal />
        <DiaryDeskModal />
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run HomeOfficeView 2>&1 | tail -15`
Expected: PASS — 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/home-office/HomeOfficeView.tsx ui/src/components/home-office/HomeOfficeView.test.tsx
git commit -m "feat(home-office): HomeOfficeView page container + modal integration"
```

---

## Task 17: LeftSidebar entry + MainArea wire-in

**Files:**
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx` (around line 1064 — right after Automations block)
- Modify: `ui/src/components/tabs/MainArea.tsx` (around line 146–183)

- [ ] **Step 1: Add LeftSidebar button under Automations**

Find this existing block in `ui/src/components/app-shell/LeftSidebar.tsx` (~line 1050–1064):

```tsx
      {mode === 'agent' && (
        <div className="px-3 pb-1">
          <button
            type="button"
            onClick={() => setAutomationPanelOpen(true)}
            className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md
                       text-[12px] text-foreground/60 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors titlebar-no-drag"
            title="Automations"
          >
            <Bot className="size-3.5 shrink-0" />
            <span className="flex-1 text-left">Automations</span>
          </button>
        </div>
      )}
```

**Add a sibling block right after it (and add `Palmtree` to the existing `lucide-react` import at the top of the file):**

```tsx
      {mode === 'agent' && (
        <div className="px-3 pb-1">
          <button
            type="button"
            onClick={() => setHomeOfficeOpen(true)}
            className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md
                       text-[12px] text-foreground/60 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors titlebar-no-drag"
            title="Home Office"
          >
            <Palmtree className="size-3.5 shrink-0" />
            <span className="flex-1 text-left">Home Office</span>
          </button>
        </div>
      )}
```

Also add at the top of the component (near the existing `automationPanelOpenAtom` usage at line ~74 and ~356):

```tsx
// Add to imports near line 74:
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'

// Add inside the component body (next to setAutomationPanelOpen):
const [, setHomeOfficeOpen] = useAtom(homeOfficePanelOpenAtom)
```

- [ ] **Step 2: Wire HomeOffice into MainArea**

In `ui/src/components/tabs/MainArea.tsx`, add the import near line 27–28:

```tsx
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { HomeOfficeView } from '@/components/home-office/HomeOfficeView'
```

Add the atom hook near line 42 (next to `automationOpen`):

```tsx
const [homeOfficeOpen, setHomeOfficeOpen] = useAtom(homeOfficePanelOpenAtom)
```

Add Esc handler (mirroring the automation one at ~167):

```tsx
React.useEffect(() => {
  if (!homeOfficeOpen) return
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') setHomeOfficeOpen(false)
  }
  window.addEventListener('keydown', onKey)
  return () => window.removeEventListener('keydown', onKey)
}, [homeOfficeOpen, setHomeOfficeOpen])
```

Modify the conditional render around line 182 — change:

```tsx
{automationOpen ? (
  automationBody
) : previewOpen ? (
```

to:

```tsx
{homeOfficeOpen ? (
  <HomeOfficeView />
) : automationOpen ? (
  automationBody
) : previewOpen ? (
```

- [ ] **Step 3: TypeScript check + existing test suites pass**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

Run: `cd ui && npm test -- --run 2>&1 | tail -15`
Expected: all existing tests still pass (no regressions).

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/app-shell/LeftSidebar.tsx ui/src/components/tabs/MainArea.tsx
git commit -m "feat(home-office): LeftSidebar entry below Automations + MainArea panel wire-in"
```

---

## Task 18: scripts/gen-home-office-sprites.sh (asset generation helper)

**Files:**
- Create: `scripts/gen-home-office-sprites.sh`

A bash script that drives Veo via `gcloud` ADC + chromakey via ffmpeg + img2webp packing. Mirrors the PetWidget pipeline. **Plan ships the script; running it requires user's gcloud credentials.**

- [ ] **Step 1: Write the script**

Create `scripts/gen-home-office-sprites.sh`:

```bash
#!/usr/bin/env bash
# gen-home-office-sprites.sh — generate Lofi-girl character sprites for HomeOffice.
#
# Pipeline per direction/pose:
#   1. Compose Veo prompt with green-screen background
#   2. Call Vertex AI Veo via REST (gcloud ADC for auth)
#   3. ffmpeg chromakey green → alpha
#   4. ffmpeg crop to square (720x720)
#   5. img2webp pack to animated WebP @ 24fps
#
# Output:  ui/public/home-office/sprites/lofi-girl/<name>.webp
#
# Requirements: gcloud CLI auth'd, ffmpeg, img2webp (brew install webp)
set -euo pipefail

PROJECT="${GCP_PROJECT:-project-ec32b5be-e193-4a7a-9c5}"
REGION="us-central1"
MODEL="veo-3-fast-generate-preview"
OUT_DIR="ui/public/home-office/sprites/lofi-girl"
TMP_DIR=".cache/home-office-sprites"
mkdir -p "$OUT_DIR" "$TMP_DIR"

ASSETS=(
  "walk-N|lofi girl walking away from camera (back view), serene focused expression, holding a small leather notebook, miyazaki ghibli watercolor style"
  "walk-NW|lofi girl walking diagonally away-and-left (3/4 back-left view), miyazaki ghibli watercolor style"
  "walk-W|lofi girl walking to the left (full side view, left profile), miyazaki ghibli watercolor style"
  "walk-SW|lofi girl walking diagonally toward-and-left (3/4 front-left view), miyazaki ghibli watercolor style"
  "walk-S|lofi girl walking toward camera (front view), miyazaki ghibli watercolor style"
  "pose-idle|lofi girl lying relaxed on a hammock with arms behind head, eyes half-closed, miyazaki ghibli watercolor style"
  "pose-thinking|lofi girl sitting cross-legged with an open book in lap, head tilted thoughtfully, miyazaki ghibli watercolor style"
  "pose-typing|lofi girl seated at a wooden writing desk, leaning forward writing in a journal with a quill, miyazaki ghibli watercolor style"
  "pose-success|lofi girl standing with both arms raised, joyful grin, miyazaki ghibli watercolor style"
  "pose-error|lofi girl seated hugging knees, head bowed, sad expression, miyazaki ghibli watercolor style"
)

BG="solid uniform vivid pure green chromakey background (#00FF00), no other elements"
STYLE="character is fully visible, head to feet, centered, no horizon line, 8 second loop, 720p"

for entry in "${ASSETS[@]}"; do
  NAME="${entry%%|*}"
  PROMPT="${entry#*|}, $BG, $STYLE"
  echo "==> $NAME"

  MP4="$TMP_DIR/$NAME.mp4"
  ALPHA_DIR="$TMP_DIR/$NAME-frames"
  WEBP_OUT="$OUT_DIR/$NAME.webp"

  if [ -f "$WEBP_OUT" ]; then
    echo "    skip (exists)"
    continue
  fi

  if [ ! -f "$MP4" ]; then
    TOKEN=$(gcloud auth print-access-token)
    RESP=$(curl -sS -X POST \
      --resolve us-central1-aiplatform.googleapis.com:443:142.250.0.95 \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      "https://us-central1-aiplatform.googleapis.com/v1/projects/$PROJECT/locations/$REGION/publishers/google/models/$MODEL:predictLongRunning" \
      -d "{\"instances\":[{\"prompt\":\"$PROMPT\"}],\"parameters\":{\"aspectRatio\":\"16:9\",\"durationSeconds\":8}}")
    OP_NAME=$(echo "$RESP" | python3 -c "import json,sys;print(json.load(sys.stdin)['name'])")
    # Poll until done
    while true; do
      sleep 8
      OP=$(curl -sS -H "Authorization: Bearer $(gcloud auth print-access-token)" \
        "https://us-central1-aiplatform.googleapis.com/v1/$OP_NAME")
      DONE=$(echo "$OP" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('done',False))")
      if [ "$DONE" = "True" ]; then
        URI=$(echo "$OP" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['response']['videos'][0]['gcsUri'])")
        gsutil cp "$URI" "$MP4"
        break
      fi
    done
  fi

  mkdir -p "$ALPHA_DIR"
  # chromakey + crop to 720x720 (centered)
  ffmpeg -y -i "$MP4" -vf \
    "chromakey=0x00FF00:0.20:0.08,crop=720:720:(in_w-720)/2:(in_h-720)/2,format=yuva420p" \
    "$ALPHA_DIR/frame-%03d.png" 2>&1 | tail -2

  # Pack to animated WebP
  img2webp -loop 0 -lossy -q 75 -m 2 -d 42 -mixed \
    $(ls "$ALPHA_DIR"/frame-*.png | sort) -o "$WEBP_OUT"

  ls -la "$WEBP_OUT"
done

echo "==> Done. Generated $(ls "$OUT_DIR"/*.webp 2>/dev/null | wc -l)/10 sprites."
```

- [ ] **Step 2: Make executable**

```bash
chmod +x scripts/gen-home-office-sprites.sh
```

- [ ] **Step 3: Verify script syntactic sanity**

Run: `bash -n scripts/gen-home-office-sprites.sh && echo OK`
Expected: prints `OK`.

- [ ] **Step 4: Commit**

```bash
git add scripts/gen-home-office-sprites.sh
git commit -m "tools(home-office): gen-home-office-sprites.sh — Veo + chromakey + img2webp pipeline"
```

---

## Task 19: Generate real Lofi-girl sprites + visual QA

**Files:**
- Modify: `ui/public/home-office/sprites/lofi-girl/*.webp` (10 generated files)
- Remove: `ui/public/home-office/sprites/lofi-girl/_placeholder.png`

This task is **manual execution + visual verification**. No code change; only assets.

- [ ] **Step 1: Run the generator**

```bash
./scripts/gen-home-office-sprites.sh
```

Expected: outputs 10 WebP files into `ui/public/home-office/sprites/lofi-girl/`. Takes ~5–15 min depending on Veo queue. If any single sprite fails (Veo rejection, quota), re-run — the script skips existing outputs.

- [ ] **Step 2: Verify all 10 assets exist + are non-empty**

Run:
```bash
ls -la ui/public/home-office/sprites/lofi-girl/
```

Expected: 10 `.webp` files, each between ~500KB and ~3MB. If any are zero-byte, delete and re-run script.

- [ ] **Step 3: Remove placeholder**

```bash
rm ui/public/home-office/sprites/lofi-girl/_placeholder.png
```

- [ ] **Step 4: Visual smoke test in dev**

```bash
cd src-tauri && cargo tauri dev
```

Expected: app launches. Click LeftSidebar `🏝️ Home Office`. Verify:

1. Scene v5 background renders
2. Character sprite appears near center (oak desk)
3. Hover over any zone → dashed gold border + light highlight appears
4. Click `🎵 Music Gazebo` → modal opens, audio plays
5. Click `📌 Sticky Wall` → modal opens, can add/remove notes
6. Click `✍️ Oak Desk` → modal opens (empty diary message)
7. Send a message in right chat panel → character walks toward oak desk + `pose-typing` plays
8. Wait for stream-complete → character does `pose-success` briefly → walks to hammock → `pose-idle`
9. Trigger an error (cancel a stream, or use a known-bad prompt) → character walks to fire pit + `pose-error`
10. Press Esc → returns to chat view
11. Sakura petals drift across screen continuously
12. 3 kodama bob along Lissajous paths

Note any discrepancies — fix in subsequent commits if necessary.

- [ ] **Step 5: Commit assets**

```bash
git add ui/public/home-office/sprites/lofi-girl/*.webp
git rm ui/public/home-office/sprites/lofi-girl/_placeholder.png ui/public/home-office/sprites/lofi-girl/.gitkeep
git commit -m "assets(home-office): Lofi-girl 5 walk + 5 pose sprites (animated WebP, 720x720)"
```

---

## Self-Review (done — issues noted below have been addressed inline)

**1. Spec coverage:**

| Spec section | Implemented by |
|---|---|
| §架构 → PixiJS + pixi-react + animated WebP | Tasks 1, 7–12 |
| §文件结构 | Tasks 2–17 |
| §状态原子 (home-office-atoms.ts) | Task 2 |
| §状态机 (Tauri events → state) | Task 6 |
| §Animator (WebP 帧驱动) | Task 7 |
| §Zone 命中 + 8 zones | Tasks 4, 9 |
| §LeftSidebar 入口 | Task 17 |
| §粒子（樱花/kodama/水雾） | Task 11 |
| §性能预算 (lazy load on view enter) | Task 8 BackgroundLayer + Task 10 character loader use Assets/loadAnimatedWebpCached (cache cleared on view destroy is a Phase 2 nicety; Phase 1 keeps cache for re-entry speed) |
| §错误处理 (WebP fallback → static texture) | Task 7 `loadAnimatedWebp` fallback path |
| §测试策略 | Tasks 2, 3, 5, 6, 7, 16 cover atoms / hooks / animator / modal-level integration |
| §音乐播放 | Task 13 |
| §便签 | Task 14 |
| §日记 | Task 15 |
| §花园 → skills navigate | Task 9 (CustomEvent dispatch) |
| §图书塔 → history navigate | Task 9 |

**2. Placeholder scan:** No "TBD" / "TODO" / "implement later" in plan body. The PIXI cleanup-on-unmount nicety is explicitly deferred to Phase 2 in the self-review table — that is a known scope decision, not a placeholder.

**3. Type consistency:** `HomeOfficeState`, `Direction`, `Vec2`, `StickyNote`, `DiaryEntry`, `Zone`, `WebpAnimator`, `loadAnimatedWebpCached`, `resolveSpriteKey`, `vectorToDirection`, `STATE_TO_ZONE` — all names match across tasks. `useHomeOfficeAgentSync` and `useCharacterPath` are both called in Task 16 with no args.

---

## PR shape (per CLAUDE.md)

One branch (`claude/home-office-phase1`), 19 commits, one PR with this commits-bisectable table:

| # | Commit | Bisect handle |
|---|---|---|
| 1 | scaffold deps + asset dirs + v5 bg | install fails / asset missing |
| 2 | atoms | state shape |
| 3 | dir-utils | vector→direction logic |
| 4 | hit-areas | zone constants |
| 5 | useCharacterPath | walk lerp + direction |
| 6 | useHomeOfficeAgentSync | IPC mapping |
| 7 | WebpAnimator + sprite-loader | frame swap + WebP decode |
| 8 | BackgroundLayer | bg PNG renders |
| 9 | ZoneLayer | zone hover/click |
| 10 | CharacterLayer | character sprite wiring |
| 11 | ParticleLayer | sakura/kodama/mist |
| 12 | HomeOfficeScene composition | scene assembly |
| 13 | MusicGazeboModal | audio modal |
| 14 | StickyNoteModal | sticky CRUD |
| 15 | DiaryDeskModal | diary read-only |
| 16 | HomeOfficeView container | page integration |
| 17 | LeftSidebar entry + MainArea wire | navigation |
| 18 | gen-home-office-sprites.sh | asset script |
| 19 | real Lofi-girl WebP assets | visual QA pass |

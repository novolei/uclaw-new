# Bottom Dock — Apex polish

**Date:** 2026-05-19
**Status:** Spec — awaiting approval, not yet planned
**Brainstorm session:** `.superpowers/brainstorm/1861-1779161516/`

## Goal

Turn the existing macOS-dock-style `BottomDock` into a **"living" dock** that:
- Has Apple's iOS 26 / macOS Tahoe "Liquid Glass" visual quality
- Reflects the AI Coworker's activity (Agent breathing, memory consolidation, streaming, etc.)
- Lets the user reorder it and pin frequently-used items
- Bounces to notify on important events (tool-approval needed, new message in non-active mode)

Scope is intentionally *contained to the dock and adjacent state machines*. The agent loop, memU bridge, and notification bus are read from — not changed.

## Non-goals

- Right-click context menus per icon (deferred)
- Streaming-rate-aware liveness (dynamic tempo) — fixed tempo only in this round
- Edge indicator while dock is collapsed — explicitly out (user prefers full clean state when hidden)
- Symphony workflow as a pinnable item — deferred to a later iteration

## Phased rollout

Three sequential PRs, each independently shippable:

| Phase | Theme | Touches | Risk |
|---|---|---|---|
| **1 · Visual** | New icon system + chrome polish | `BottomDock.tsx`, `DockItem.tsx`, `ConnectionIndicator.tsx`, new asset folder | Low (presentation only) |
| **2 · Structure** | Reorder + Pin to Dock | `dock-atoms.ts` (data model), `BottomDock.tsx`, new `DockPinSection.tsx` | Medium (new persisted state) |
| **3 · Liveness** | Breathing + particles + memory pulse + bounce | New `useDockLiveness` hook, `DockItem.tsx`, event-bus subscriptions | Medium (cross-system signal wiring) |

---

## Phase 1 · Visual polish

### 1.1 Icon system

9 brand icons in macOS Sequoia / iOS 26 **"Liquid Glass"** style. Each is a 1024×1024 transparent PNG with the squircle filling the entire frame edge-to-edge:

| Slot | File | Symbol | Gradient |
|---|---|---|---|
| Chat *(dock)* | `chat.png` | single rounded speech bubble | sky-blue → cyan → teal |
| Agent *(dock)* | `agent.png` | 4-point Apple-Intelligence sparkle | indigo → violet → magenta |
| Memory *(dock)* | `memory.png` | multi-faceted crystalline gem | amber → coral → rose |
| Kaleidoscope *(dock)* | `kaleidoscope.png` | 6-petal radial flower w/ rainbow refraction | violet → magenta → pink |
| Connections | `connections.png` | 3-node triangle network | teal → cyan → blue |
| Home Office | `home-office.png` | warm desk lamp with cast glow | amber → orange → red-orange |
| Humane *(automation)* | `humane.png` | heart embraced by a gear | emerald → green → lime |
| Alert | `alert.png` | tilted notification bell with motion arcs | golden-yellow → orange → red-orange |
| Settings | `settings.png` | 3 stacked horizontal sliders | slate-blue → indigo → violet |

**Asset location:** `ui/src/assets/dock-icons/*.png`. Vite hashes them through its asset pipeline so the bundler produces cache-busting URLs at build time. Import as: `import chatIcon from '@/assets/dock-icons/chat.png'`.

**Color note:** these icons intentionally do *not* follow theme `--primary`. Each has its own permanent color identity (like macOS apps — Music is always red, Mail is always blue). They are designed to read on all 11 uClaw themes; the squircle gradient is rich enough that no per-theme variant is needed.

### 1.2 Active state indicator

The current implementation wraps the active icon in a primary-tinted gradient pill + ring + glow. **Remove all of that.** Each icon is now visually distinctive on its own; an extra slot decoration would compete with the icon's color identity.

Replacement: a single 4×4 px primary-tinted dot positioned 8 px below the active icon. Slight `0 0 6px hsl(var(--primary) / 0.5)` outer glow. No hover backplate either — the icon itself is the hit target; magnification handles hover feedback.

### 1.3 Icon display size

Current dock renders icons at 18 px inside a 38 px slot. The Liquid Glass detail demands more pixels.

- `SLOT_W` constant in [DockItem.tsx](ui/src/components/dock/DockItem.tsx#L33): **48 → 56**
- `ICON_BOX` constant: **38 → 44**
- Rendered icon size: **18 → 28** (`<img width={28} height={28}>` replacing the lucide icon)

The dock body grows by ~10 px in width per item — acceptable for the canvas Liquid Glass needs to breathe.

### 1.4 Drag handle (preview)

A non-interactive visual hint that the dock supports reordering/pinning (Phase 2 wires the actual drag behavior; Phase 1 only ships the affordance).

4 small horizontal dots, 4 px diameter each, centered above the dock body. Opacity 0 by default; fades to 0.55 over 150 ms when the user's cursor enters the dock body. Goes back to 0 on leave.

### 1.5 Connection status indicator

Replace the current 3 sage/coral/amber dots (`ConnectionIndicator.tsx`) with a 3-bar signal-strength indicator:

- 3 vertical bars, increasing height (6 / 10 / 14 px), 3 px wide, 2 px gap
- Bar 1 = Internet, Bar 2 = backend (Tauri local API), Bar 3 = memU bridge
- Color per bar: sage when online, amber on initialization, coral when offline
- Hover the cluster → existing Radix Tooltip stack (one per bar) still surfaces the per-channel state text

Behaviorally identical to today; visually replaced.

---

## Phase 2 · Structure

### 2.1 Dock data model

Today `BottomDock` has a hardcoded `NAV_ITEMS` array. Phase 2 makes the dock data-driven via persistent atoms.

```ts
// dock-atoms.ts (new fields)
type DockItemSpec =
  | { kind: 'mode'; mode: 'chat' | 'agent' | 'memory' | 'kaleidoscope' }
  | { kind: 'pinned-conversation'; sessionId: string; type: 'chat' | 'agent' }
  | { kind: 'pinned-workspace'; spaceId: string }
  | { kind: 'pinned-automation'; specId: string }

export const dockOrderAtom = atomWithStorage<DockItemSpec[]>('dock:order', [
  { kind: 'mode', mode: 'chat' },
  { kind: 'mode', mode: 'agent' },
  { kind: 'mode', mode: 'memory' },
  { kind: 'mode', mode: 'kaleidoscope' },
])
```

The active list is rendered by `BottomDock`; reorder mutates this atom; pin adds entries; unpin removes them.

### 2.2 Reorder (drag-to-reorder)

- **Library:** `@dnd-kit/core` + `@dnd-kit/sortable` (used elsewhere in uClaw already — verify and reuse; if absent, add). Headless dnd, accessible, supports keyboard.
- **Trigger:** long-press (200 ms) on any dock item OR grab the drag-handle dots — both work.
- **Scope:** *Everything is reorderable*. Modes can swap with pinned items; pins can swap with modes. No protected slots.
- **Visual:** classic macOS feel — dragged item lifts slightly (scale 1.05), siblings *fluidly shift* to make room with a spring animation (300/30 stiffness/damping), drop snaps to slot.
- **Persistence:** order writes to `dockOrderAtom` on drop.

### 2.3 Pin to Dock

#### What can be pinned

| Type | Source action | Display |
|---|---|---|
| Conversation | from chat/agent session list: right-click → Pin | the conversation's title initials in a colored squircle, OR the conversation's color seed |
| Workspace (Space) | from `LeftSidebar` workspace list: right-click → Pin | the workspace's emoji / first-letter |
| Automation spec | from automation list: right-click → Pin | the spec's emoji / icon |

The pinned dock items need their own *icon rendering strategy* — these don't have a Liquid Glass squircle PNG, since they're user-generated. Proposal: render them as a 44 px squircle backplate (CSS-only) using the entity's color seed, with the entity's emoji / initials in white. Visually adjacent to but not identical to the brand icons — clear "this is a shortcut, not a mode" affordance.

#### Where pins sit

Pins live in a **separate "pinned" section** after the 4 mode icons + a 1 px vertical divider:

```
[Chat] [Agent] [Memory] [Kaleidoscope] │ [pin1] [pin2] [pin3] │ [signal-bars]
                                       ↑                       ↑
                                       divider (border-border) divider
```

But per the reorder rule "全部可重排", the divider is *visual only* — it can be crossed during drag. The dock recomputes the divider's render position based on whether each item is `kind: 'mode'` vs `kind: 'pinned-*'`. If a user reorders a mode into the pinned section, the divider follows.

(Open: does this contradict "kind-aware sectioning"? **Resolution**: divider is *cosmetic*. It separates the first contiguous run of `kind:'mode'` items from everything else. Reorder is purely positional; sections are derived from kinds.)

#### Max pins / overflow

Soft cap of 8 pinned items. Beyond 8, an overflow button (`…`) appears that opens a popover with the rest. This is Phase-2-stretch — first release ships with no cap and a horizontal scroll if dock width exceeds viewport.

### 2.4 Bounce on event

When a trigger fires:

1. If dock is collapsed → first **reveal it** (slide up with the existing show animation)
2. Then **bounce the target icon** — spring scale 1 → 1.35 → 1 with overshoot, ~500 ms total
3. Hold visible for **1.5 s** so the user can decide to interact
4. Auto-hide normally (180 ms debounced as today)

Total event-to-quiet: ~2.2 s. Fast enough not to derail; slow enough to register peripherally.

#### Bounce triggers

| Event | Source signal | Target icon | Severity |
|---|---|---|---|
| Tool-call approval needed | existing `pending_approvals` map / `approve_tool_call` IPC event | Agent | **High** — bigger bounce (1.45 scale), repeats twice |
| Non-active mode receives new message | `new_message` IPC event from agent/chat backend | Chat *or* Agent depending on event mode | Normal — single bounce |

(Out of scope this round: Agent task complete, Automation escalation. They can attach to the same machinery in a later iteration.)

---

## Phase 3 · Liveness

### 3.1 The hidden-dock rule

> **When the dock is hidden, no liveness is rendered. Anywhere on the screen. Nothing.**

This is intentional. We considered an edge-glow strip, an auto-peek, and a corner indicator. All were rejected as visual noise. The dock's whole value proposition is "out of sight when not needed" — liveness shouldn't undermine it.

The only exception is **bounce-on-event** (see 2.4), which is consent-driven by an explicit signal, not background activity.

### 3.2 Visible-dock liveness signals

Three liveness signals, each tied to a single backend status:

| Signal | Trigger | Visual | Tempo |
|---|---|---|---|
| **Agent breathing ring** | `agentSessionStateAtom.activeTasks.length > 0` | A soft 4-stop radial gradient halo around the Agent icon (`0 0 14px hsl(var(--primary) / 0.45)`), opacity oscillates 0.4 → 0.8 → 0.4 | **2.0 s** loop |
| **Streaming particles** | `agentStreamingAtom === true` | 3 tiny dots emit from the top edge of Agent icon, rise 12 px while fading out over 600 ms | one particle every **400 ms** |
| **Memory pulse** | `memuConsolidatingAtom === true` (new — see 3.3) | Memory icon performs a subtle scale 1 → 1.04 → 1 wave | **1.5 s** loop |

All three respect `prefers-reduced-motion` (they stop animating; the static state remains visually distinct via subtle opacity).

### 3.3 Wiring backend signals

These atoms drive Phase 3:

| Atom | Source |
|---|---|
| `agentSessionStateAtom.activeTasks` | already exists — populated by agent loop IPC events |
| `agentStreamingAtom` | already exists — toggled by `agent_token_chunk` event handler |
| `memuConsolidatingAtom` | **new** — needs a memU bridge event (`memu_consolidation_started` / `_finished`). Falls back to `false` if the event is never emitted (degrades gracefully) |

`useDockLiveness` is a new hook that consumes the three atoms and returns a `LivenessState` per dock item. `DockItem` reads the relevant flag and renders the visual.

---

## Architecture impact

### Atoms (new / changed)

```
ui/src/atoms/dock-atoms.ts
+ dockOrderAtom              (atomWithStorage)
+ memuConsolidatingAtom      (atom<boolean>)
~ existing atoms unchanged
```

### Components

```
ui/src/components/dock/
~ BottomDock.tsx              (data-driven render; remove hardcoded NAV_ITEMS; add divider; mount dnd-kit context)
~ DockItem.tsx                (image-based icon; remove slot bg/ring; add active dot; reorder hooks; liveness halo)
~ ConnectionIndicator.tsx     (replace 3 dots with 3 signal bars)
+ DockDragHandle.tsx          (4-dots top-center, hover-fade)
+ DockPinnedItem.tsx          (renders pinned conversation/workspace/automation as a CSS squircle)
+ useDockLiveness.ts          (subscribe to agent/memU state, return per-item liveness)
+ useDockBounce.ts            (subscribe to bounce-trigger events, drive the bounce + auto-reveal)
+ index.ts                    (named exports)

ui/src/components/app-shell/AppShell.tsx
~ wire useDockBounce into the existing BottomDockHoverRegion (so bounce can force-reveal)
```

### Assets

```
ui/src/assets/dock-icons/         (NEW — 9 PNG files, transparent 1024×1024)
  chat.png agent.png memory.png kaleidoscope.png
  connections.png home-office.png humane.png alert.png settings.png
```

Total committed weight: ~8.5 MB raw PNG. Vite's asset pipeline emits hashed copies; we should also build a **256×256 webp** sibling at build time and serve that for the dock (which renders at 28 px — 256 is plenty). Webp is ~3 KB each, total ~30 KB delivered. Implementation: a Vite plugin or a build-time script that runs during `npm run build` and emits `*.webp` alongside `*.png`. Phase 1 can ship with the PNG directly; webp optimization in Phase 1.5 if delivery weight becomes a concern.

### IPC events (new)

```
src-tauri/src/memu/client.rs
+ emit 'memu_consolidation_started' / 'memu_consolidation_finished' events around
  the consolidation routine (single-line additions in the existing methods)
```

No new Tauri commands needed; the event channel is already established.

### Compose with existing BottomDockHoverRegion

The hide/reveal state machine in `BottomDockHoverRegion` (PRs #225/#227) stays. Phase 2/3 hook into it:
- Bounce events call a new exposed `forceReveal()` from `useDockBounce`
- The 1.5 s linger is handled inside the bounce hook (clears the hide timer, schedules a new one)

---

## Testing strategy

- **Existing 23 dock tests must keep passing** with no behavior regressions.
- New tests:
  - `DockItem.test.tsx`: image renders, active dot appears for `isActive` only, no slot decoration in DOM
  - `dockOrderAtom.test.ts`: order persists to localStorage, default seed is the 4 modes
  - `DockPinnedItem.test.tsx`: renders entity squircle correctly
  - `useDockLiveness.test.tsx`: each backend atom drives the right visual flag
  - `useDockBounce.test.tsx`: trigger event → forceReveal called → bounce target set → 1.5 s timer cleanup
- **No new e2e tests** in this round — the dock is reachable from `cargo tauri dev` manually.

---

## Migration / rollout

The existing dock-enabled atom defaults to `false`. Users have to opt in. The new visuals show up immediately on opt-in. Pinned items + custom order start empty; user adds them. No DB migration needed (everything lives in localStorage atoms or in the existing IPC stream).

Bisectable commits within each phase will be enumerated in the corresponding implementation plan.

---

## Open questions (to resolve during planning)

1. **Bounce auto-reveal vs user's `bottomDockEnabledAtom`**: if user has the dock toggled off entirely, bounce should *not* override that. Confirm: bounce only auto-reveals when dock is enabled-but-hidden.
2. **Liveness signal source for `memuConsolidatingAtom`**: the memU Python bridge needs an event emitter for consolidation start/finish. If memU is unavailable on this checkout (gitignored), the atom defaults to `false` and Memory pulse never plays — graceful degradation. Phase 3 PR description must call this out.
3. **dnd-kit dependency presence**: check `ui/package.json` first; if missing, add `@dnd-kit/core` + `@dnd-kit/sortable` (~12 KB gzipped) as part of Phase 2.
4. **Pinned-item rendering for entities with no color seed**: Some old conversations may lack a `color` field. Fall back to a deterministic hash-to-color function on `sessionId`.

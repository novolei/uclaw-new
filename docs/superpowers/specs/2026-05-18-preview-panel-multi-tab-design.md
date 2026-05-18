# Preview panel multi-tab — design

**Status:** Design v1, brainstorming gate passed 2026-05-18.
**Base:** `main` at `450c328`.

## Problem

uClaw's file preview panel currently shows one file at a time via a single
`selectedPreviewFileAtom: PreviewFileTarget | null` slot. Each new "open"
clobbers the previous file. The user wants multi-file context — multiple
files open as tabs, switchable, with files opened by the agent taking
priority position.

The Agent View already has a battle-tested multi-tab pattern for chat
sessions (`tabsAtom` + `workspaceActiveTabIdMapAtom` + `TabBar.tsx`).
The preview panel should mirror the spirit (in-memory pool + active
pointer + chrome-style tabs) without literally reusing the workspace-
scoped session-tab component, which is too coupled to session lifecycle.

## Goals

1. Multiple files openable as tabs in the preview panel.
2. Files opened from agent tool writes / `useGlobalAgentListeners`
   are *source: agent* — they sort to the left (index 0+) and carry a
   visual marker (✨) that distinguishes them from manually-opened tabs.
3. Re-opening an already-open file (whether via FilePathChip click or
   another agent write) focuses the existing tab — never duplicates.
4. Tab switch / close behave like every other multi-tab UI: click to
   activate, X to close, closing active selects neighbor, closing the
   last tab also closes the panel.
5. In-memory state only — matches existing `tabsAtom` (chat-session tabs)
   behavior so app restart starts fresh.

## Non-goals

- Drag-to-reorder tabs (defer; Agent View tabs don't support it either).
- Tab persistence across app restart.
- Per-tab scroll-position memory (defer).
- Split-pane / side-by-side comparison view.
- Tab pinning / max-tab limits / LRU eviction.
- Same-file-multiple-tabs (different scroll positions / regions).

## Architecture

```
┌──────────── PreviewPanel ──────────────────────────────┐
│ ┌── PreviewTabBar (NEW) ──────────────────────────┐    │
│ │ ✨ index.html  ✨ styles.css │ README.md  ✕     │    │
│ │ └── agent ──┘ └── agent ──┘  └─ manual ─┘       │    │
│ └─────────────────────────────────────────────────┘    │
│ ┌── PreviewContent (existing) ────────────────────┐    │
│ │ renders active tab's file via existing renderer │    │
│ │ chain (markdown / code / image / etc.)          │    │
│ └─────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────┘
```

The split-ratio + open/close logic of the panel itself is unchanged.
Only the inner content gets tab-wrapped.

## Data model

### New atoms (in `ui/src/atoms/preview-panel-atoms.ts`)

```ts
export type PreviewTabSource = 'agent' | 'manual'

export interface PreviewTabItem {
  // Identity — composite key (mountId + relPath) ensures same file ≡ same tab
  mountId: string
  relPath: string
  // Display
  name: string
  absolutePath: string
  sessionId?: string  // optional; helps tag tabs to a session for cleanup later
  // Source-based ordering + visual marker
  source: PreviewTabSource
  addedAt: number     // tiebreaker within source group
}

/** Pool of open preview tabs. In-memory only. */
export const previewTabsAtom = atom<PreviewTabItem[]>([])

/** Active tab key (`${mountId}:${relPath}`) or null if no tab is active. */
export const activePreviewTabKeyAtom = atom<string | null>(null)
```

### Tab key helper

```ts
export function previewTabKey(
  t: Pick<PreviewTabItem, 'mountId' | 'relPath'>,
): string {
  return `${t.mountId}:${t.relPath}`
}
```

### Sort rule

```ts
function sortPreviewTabs(tabs: PreviewTabItem[]): PreviewTabItem[] {
  return [...tabs].sort((a, b) => {
    if (a.source !== b.source) return a.source === 'agent' ? -1 : 1
    return a.addedAt - b.addedAt
  })
}
```

Agent tabs always cluster left; within each group, oldest-added is leftmost
(so a fresh agent write inserts to the right of existing agent tabs, just
left of the first manual tab).

### Selected-file (derived, backward-compat)

```ts
export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(get => {
  const key = get(activePreviewTabKeyAtom)
  if (!key) return null
  const tab = get(previewTabsAtom).find(t => previewTabKey(t) === key)
  if (!tab) return null
  return {
    mountId: tab.mountId,
    relPath: tab.relPath,
    name: tab.name,
    absolutePath: tab.absolutePath,
    sessionId: tab.sessionId,
  }
})
```

Existing `PreviewPanel` and any other reader of `selectedPreviewFileAtom`
keep working unchanged.

## Actions

### `openPreviewTabAction`

Single entry point for both agent-source and manual-source opens:

```ts
export const openPreviewTabAction = atom(
  null,
  (get, set, payload: {
    target: PreviewFileTarget
    source: PreviewTabSource
  }) => {
    const tabs = get(previewTabsAtom)
    const key = previewTabKey(payload.target)
    const existing = tabs.find(t => previewTabKey(t) === key)
    if (existing) {
      // Focus existing — no duplicate, no insert
      set(activePreviewTabKeyAtom, key)
      // Source promotion: manual tab gets re-opened by agent → promote to agent
      // so it migrates to the left cluster (keeps "agent things are visible").
      if (payload.source === 'agent' && existing.source === 'manual') {
        set(previewTabsAtom, sortPreviewTabs(
          tabs.map(t => previewTabKey(t) === key ? { ...t, source: 'agent' } : t)
        ))
      }
      set(previewPanelOpenAtom, true)
      return
    }
    const tab: PreviewTabItem = {
      mountId: payload.target.mountId,
      relPath: payload.target.relPath,
      name: payload.target.name,
      absolutePath: payload.target.absolutePath,
      sessionId: payload.target.sessionId,
      source: payload.source,
      addedAt: Date.now(),
    }
    set(previewTabsAtom, sortPreviewTabs([...tabs, tab]))
    set(activePreviewTabKeyAtom, key)
    set(previewPanelOpenAtom, true)
  }
)
```

### `closePreviewTabAction`

```ts
export const closePreviewTabAction = atom(
  null,
  (get, set, key: string) => {
    const tabs = get(previewTabsAtom)
    const idx = tabs.findIndex(t => previewTabKey(t) === key)
    if (idx === -1) return
    const next = tabs.filter(t => previewTabKey(t) !== key)
    set(previewTabsAtom, next)
    if (get(activePreviewTabKeyAtom) === key) {
      // Closed the active tab → activate neighbor
      const neighbor = next[idx] ?? next[idx - 1] ?? null
      set(activePreviewTabKeyAtom, neighbor ? previewTabKey(neighbor) : null)
      if (next.length === 0) {
        set(previewPanelOpenAtom, false)
      }
    }
  }
)
```

### Compatibility wrapper

The existing `openPreviewAction` stays as a manual-source shorthand:

```ts
export const openPreviewAction = atom(
  null,
  (_get, set, target: PreviewFileTarget) =>
    set(openPreviewTabAction, { target, source: 'manual' })
)
```

So any caller that hasn't migrated still works (and defaults to manual
positioning).

## UI components

### `PreviewTabBar.tsx` (new)

Light wrapper rendering the tabs:

```tsx
export function PreviewTabBar(): React.ReactElement | null {
  const tabs = useAtomValue(previewTabsAtom)
  const activeKey = useAtomValue(activePreviewTabKeyAtom)
  const setActive = useSetAtom(activePreviewTabKeyAtom)
  const closeTab = useSetAtom(closePreviewTabAction)
  if (tabs.length === 0) return null
  return (
    <div
      className="flex items-stretch border-b border-border bg-card overflow-x-auto"
      role="tablist"
      aria-label="Preview file tabs"
    >
      {tabs.map(tab => {
        const key = previewTabKey(tab)
        const isActive = key === activeKey
        return (
          <PreviewTabItem
            key={key}
            tab={tab}
            isActive={isActive}
            onActivate={() => setActive(key)}
            onClose={() => closeTab(key)}
          />
        )
      })}
    </div>
  )
}
```

### `PreviewTabItem.tsx` (new)

Per-tab button:

```tsx
interface Props {
  tab: PreviewTabItem
  isActive: boolean
  onActivate: () => void
  onClose: () => void
}

export function PreviewTabItem({ tab, isActive, onActivate, onClose }: Props) {
  const Icon = getFileTypeIcon(tab.name)  // reuse existing FileTypeIcon palette
  return (
    <div
      role="tab"
      aria-selected={isActive}
      onClick={onActivate}
      onAuxClick={(e) => { if (e.button === 1) onClose() }}  // middle-click close
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 text-xs cursor-pointer',
        'border-r border-border/40 min-w-[80px] max-w-[180px]',
        isActive
          ? 'bg-content-area text-foreground border-b-2 border-b-primary'
          : 'bg-card text-muted-foreground hover:bg-muted/40',
      )}
    >
      {tab.source === 'agent' && (
        <span aria-label="opened by agent" title="opened by agent">✨</span>
      )}
      <Icon className="size-3.5 shrink-0" />
      <span className="truncate flex-1">{tab.name}</span>
      <button
        type="button"
        onClick={(e) => { e.stopPropagation(); onClose() }}
        className={cn(
          'size-4 flex items-center justify-center rounded',
          'opacity-0 group-hover:opacity-100',
          isActive && 'opacity-100',
          'hover:bg-muted/60',
        )}
        aria-label={`close ${tab.name}`}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}
```

### `PreviewPanel.tsx` (modify)

Insert `<PreviewTabBar />` above the existing content:

```tsx
return (
  <div className="flex flex-col h-full">
    <PreviewTabBar />          {/* NEW */}
    <PreviewContent />         {/* existing renderer chain, reads selectedPreviewFileAtom */}
  </div>
)
```

## Integration points

### Agent-side auto-open

`ui/src/hooks/useGlobalAgentListeners.ts` currently calls `openPreviewAction(target)`
when an agent tool's `tool_result` resolves with a writable path. Change to:

```ts
set(openPreviewTabAction, { target, source: 'agent' })
```

### Manual click

`ui/src/components/preview/chips/FilePathChip.tsx` currently calls
`openPreviewAction(target)`. Change to:

```ts
set(openPreviewTabAction, { target, source: 'manual' })
```

### Other callers

Grep for all `openPreviewAction(` callers + any direct `set(selectedPreviewFileAtom, ...)`.
For each:
- If the call originates in a user-initiated action (button click) → `source: 'manual'`
- If from agent stream / background event → `source: 'agent'`

The compatibility wrapper makes pure clobber-style callers default to manual.

## Edge cases

- **Last tab closed** → panel closes (`previewPanelOpenAtom = false`). Re-opening any file re-shows the panel with one tab.
- **Active tab's file deleted on disk** → handled by existing renderer (shows error / placeholder). Tab stays open until user closes it.
- **Refresh button** (if exists on the panel header) → applies to active tab only.
- **Same name, different paths** → both render with the same `name` but their composite `mountId:relPath` keys are unique, so they're separate tabs. Visual disambiguation (showing dir prefix when names collide) is deferred.
- **Panel collapsed via `previewPanelOpenAtom = false`** + new agent write → `openPreviewTabAction` re-opens the panel and inserts the new tab.

## Testing

- **Unit (`previewTabsAtom` action tests)** in `ui/src/atoms/preview-panel-atoms.test.ts`:
  - `openPreviewTabAction` inserts a new tab, sets active, opens panel
  - Re-opening the same file (same key) does NOT add a duplicate, just sets active
  - manual → agent re-open promotes the source and re-sorts
  - `closePreviewTabAction` removes the tab, activates neighbor when closing active
  - Closing last tab also closes the panel
  - Sort order: agent tabs cluster left, manual right, both by addedAt
- **RTL (`PreviewTabBar.test.tsx`)**:
  - Empty state: renders nothing when 0 tabs
  - Renders correct count + active highlight
  - Close X click dispatches closePreviewTabAction
  - Middle-click on tab triggers close
  - Agent-source tab shows the ✨ marker
- **Integration**:
  - Clicking FilePathChip when no tabs open → new tab + active + panel open
  - Clicking same FilePathChip again → same tab focused, no duplicate
  - Agent auto-open with source='agent' inserts to the left of existing manual tabs

## File map

| File | Action |
|---|---|
| `ui/src/atoms/preview-panel-atoms.ts` | MODIFY: add atoms + actions, derive `selectedPreviewFileAtom`, wrap `openPreviewAction` |
| `ui/src/atoms/preview-panel-atoms.test.ts` | NEW (or extend existing) |
| `ui/src/components/preview/PreviewTabBar.tsx` | NEW |
| `ui/src/components/preview/PreviewTabItem.tsx` | NEW |
| `ui/src/components/preview/PreviewTabBar.test.tsx` | NEW |
| `ui/src/components/preview/PreviewPanel.tsx` | MODIFY: mount `<PreviewTabBar>` above content |
| `ui/src/hooks/useGlobalAgentListeners.ts` | MODIFY: agent-source calls |
| `ui/src/components/preview/chips/FilePathChip.tsx` | MODIFY: manual-source calls |

## Risks / open questions

1. **Other `openPreviewAction` callers I don't know about** — must grep exhaustively at implementation time. Listed in Task 3 of plan.
2. **Same-file double tab via different `mountId` representations** — if the same absolute path resolves to different mountIds (e.g., relative vs absolute mount), the key dedup misses. Acceptable; would surface as 2 tabs for the same file, no functional break.
3. **Renderer chain stability** — `PreviewContent` reads `selectedPreviewFileAtom` (now derived). When the active tab changes, the renderer re-mounts with the new target. Some renderers (e.g., monaco editor) might have flicker. Verified during integration testing.

## Implementation order (input to writing-plans)

1. New atoms + actions + unit tests (action-level, no UI)
2. Compatibility wrapper for `openPreviewAction` + derived `selectedPreviewFileAtom`
3. New `PreviewTabBar` + `PreviewTabItem` components + RTL tests
4. Mount `<PreviewTabBar>` in `PreviewPanel`
5. Migrate `useGlobalAgentListeners` to source='agent'
6. Migrate `FilePathChip` to source='manual'
7. Exhaustive grep for any remaining `openPreviewAction(` / `set(selectedPreviewFileAtom, ...)` and migrate
8. Integration test: agent write → tab insert left of manual tabs

writing-plans will turn these into bisectable per-commit tasks.

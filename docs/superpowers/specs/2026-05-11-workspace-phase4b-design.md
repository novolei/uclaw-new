# Workspace Phase 4b — ARC-style Workspace Switcher

**Status**: Spec
**Date**: 2026-05-11
**Author**: Ryan + Claude (uClaw repo)
**Phase**: 4b of 4 (final phase)
**Prerequisite**: [Phase 4a spec](./2026-05-11-workspace-phase4a-design.md) — PR #78 (in-flight). Phase 4b builds on Phase 4a's `TabBarWorkspaceChip` (downgrades it) and `WorkspaceCreateDialog` (reuses).

---

## 1. Background

The current workspace navigation in uClaw uses a **tree-of-all-workspaces** model: the left sidebar (`WorkspaceRail`) shows every workspace as a collapsible group with its sessions nested inside. Switching workspaces means scrolling/expanding the tree.

This works but has three structural problems:

1. **Visual clutter at scale**: with 5+ workspaces and 10+ sessions each, the sidebar tree becomes a dense thicket. Finding a specific session in the active workspace requires scrolling past all the others.
2. **No workspace-level glance state**: there's no single anchor showing "you are here". Phase 4a's `TabBarWorkspaceChip` partially solves this but has a redundant dropdown that duplicates the tree's switching functionality.
3. **Workspace-level management is hidden inside the tree**: rename and delete live on hover affordances on each workspace's header in the tree. They're discoverable but not prominent for the *active* workspace.

Phase 4b adopts the ARC browser's **Space-switcher** pattern: the sidebar shows only the **active** workspace's sessions; switching workspaces happens via a bottom icon bar; workspace-level management gets its own dedicated header at the top of the sidebar.

This is the final phase of the workspace remediation series. Phases 1-3 fixed the data model and agent sandbox; Phase 4a added shortcuts + create-modal polish. Phase 4b reshapes the navigation surface itself.

## 2. Goals

1. **`WorkspaceSwitcherBar`**: new bottom-of-sidebar bar `[automation] | [workspace icons OR dots] | [+]` with 1px dividers between zones. ≤5 workspaces show full emoji icons; >5 collapses non-active workspaces to 6px dots; only the active workspace keeps its full icon. Drag-reorder works on both icons and dots. Each icon/dot has a running-session pulse indicator when sessions in that workspace are executing. Hover tooltip uses ARC-style pill chips for `name + ⌘ + digit` (not plain text).
2. **`WorkspaceHeader`**: new top-of-sidebar element showing active workspace's emoji + name + truncated path with hover ✏ rename and 🗑 delete buttons. Default workspace shows the read-only view (no buttons).
3. **`WorkspaceRail` rewrite**: renders ONLY the active workspace's sessions as a flat list. Deletes the per-workspace tree iteration and the standalone "+ 新建工作区" button (moved to `WorkspaceSwitcherBar`'s `+`).
4. **`TabBarWorkspaceChip` downgrade** (Phase 4a → 4b): becomes a passive label (emoji + truncated name, no chevron, no dropdown). Still useful as the only workspace indicator when the sidebar is collapsed.
5. **Per-workspace right-panel tab memory**: new `workspaceActiveRightPanelTabMapAtom: Map<workspaceId, ActiveTab>` so each workspace remembers its last viewed Files/Teams/Plan/Trajectory/Browser tab.
6. **User+Settings row stays at bottom**, BELOW the switcher bar — workspace switcher is per-app context; user/settings is cross-workspace global identity.
7. **Remove the sidebar collapse feature entirely**: delete the `sidebarCollapsedAtom`, the `toggle-sidebar` (Cmd+B) shortcut, the collapse button UI, and the entire collapsed-mode render branch in `LeftSidebar.tsx`. Sidebar is always expanded. With the new switcher bar fitting at bottom, vertical space loss is minimal; the collapse mode was vestigial.
8. **Backend untouched**. Reuses Phase 1-3 IPCs. No migrations.
9. **Build green at every commit** — 5 commits, single PR.
10. **Tests**: ~20 Vitest cases across 4 new/updated test files; ~5 deleted with `WorkspaceGroup`.

## 3. Non-Goals (deferred / out of scope)

- **Per-workspace theme color tint**: ARC tints the entire sidebar with each Space's color. We defer — uClaw's themes are global; per-workspace theming requires a new data model (workspace.theme column or atom) and conflicts with the existing 11-theme system.
- **In-sidebar create form**: ARC's "Create a Space" replaces the sidebar contents temporarily. We keep the modal `WorkspaceCreateDialog` (Phase 4a). Same functionality, simpler interaction model.
- **Per-workspace last-active-session memory**: would persist "when I return to workspace A, restore the session I was last viewing". Out of scope; relies on existing per-session active-tab atoms already in place.
- **Disk persistence of `workspaceActiveRightPanelTabMapAtom`**: in-memory only. App restart defaults all workspaces to Files tab. Adding disk persistence is straightforward but not Phase 4b scope.
- **Background-session notification badges**: a count of completed/failed tool calls per workspace. The pulse-dot indicator only signals "something is running"; not history. Defer.
- **Sidebar collapse mode**: deleted entirely (Goal #7). No "narrow sidebar with switcher bar" interaction to design.
- **Workspace search input in switcher**: an input field inside the switcher dropdown to filter workspaces by name. Not needed for <20 workspaces; defer.
- **Per-workspace icon picker beyond emoji**: ARC supports custom upload. We stay with the 8 default emojis from Phase 4a.

## 4. Detailed Design

### 4.1 `WorkspaceSwitcherBar`

**File**: `ui/src/components/workspace/WorkspaceSwitcherBar.tsx` (new)

**Layout**:
```
┌─ bottom bar (h-12, px-2, py-1.5) ─────────────────────────────┐
│ [🤖]  │  [📁][💼][🚀][🔬][🎯]  │  [+]                          │
│  zone1   ───── zone2 ─────       zone3                          │
└─────────────────────────────────────────────────────────────────┘
```

**Render rules**:
```ts
const FULL_THRESHOLD = 5

if (workspaces.length <= FULL_THRESHOLD) {
  workspaces.map(w => <WorkspaceIcon ws={w} active={w.id === activeId} />)
} else {
  workspaces.map(w => w.id === activeId
    ? <WorkspaceIcon ws={w} active />
    : <WorkspaceDot ws={w} />
  )
}
```

**`WorkspaceIcon`**:
- Button 24×24, rounded-md, contains 14px emoji centered.
- Active: `ring-2 ring-primary ring-offset-1 bg-primary/10`.
- Hover: `bg-foreground/[0.06]`.
- Running indicator: absolute top-right 6px pulse dot `bg-primary animate-pulse shadow-[0_0_4px_hsl(var(--primary))]`.
- onClick → `selectWorkspaceAtom(w.id)`.
- draggable=true.

**`WorkspaceDot`** (collapsed mode only):
- Button 12×12 hit area, visually 6px circle, `bg-foreground/30`.
- Active: not applicable (active workspace always renders as full icon).
- Hover: `bg-foreground/50` + slight size scale.
- Running indicator: smaller 3px dot, no glow.
- Same drag behavior.

**Shared tooltip — `WorkspaceTooltip` sub-component** (ARC-style):

```tsx
function WorkspaceTooltip({
  workspace, indexForShortcut,
}: { workspace: WorkspaceInfo; indexForShortcut: number | null }) {
  return (
    <div className="flex items-center gap-1.5 px-2 py-1 rounded-md
                    bg-popover/95 backdrop-blur-md border border-border/60
                    shadow-lg text-[12px] font-medium">
      <span className="leading-none text-[13px]">{workspace.icon}</span>
      <span className="text-foreground">{workspace.name}</span>
      {indexForShortcut !== null && indexForShortcut < 9 && (
        <>
          <span className="px-1.5 py-0.5 rounded bg-primary/15 text-primary
                           text-[10px] font-mono leading-none">
            {isMac ? '⌘' : 'Ctrl'}
          </span>
          <span className="px-1.5 py-0.5 rounded bg-primary/15 text-primary
                           text-[10px] font-mono leading-none">
            {indexForShortcut + 1}
          </span>
        </>
      )}
    </div>
  )
}
```

Used inside Radix `<Tooltip>` for both `WorkspaceIcon` and `WorkspaceDot`. Sits `side="top"` with `sideOffset={6}`. The pill style matches the ARC reference visually:
- Workspace name as plain text on left.
- Two small pills on right: modifier (`⌘` or `Ctrl`) + digit, each with `bg-primary/15` background and `text-primary` foreground.
- For workspaces beyond #9 (no shortcut), the right-side pills are omitted; tooltip shows only emoji + name.
- Theme tokens throughout (`bg-popover`, `text-foreground`, `text-primary`) so the tooltip survives all 11 themes.

**Automation button (zone 1)**: extracted from existing `LeftSidebar.tsx:790-815` logic. Reuses `setAutomationPanelOpen` and `AutomationSlideOver`.

**`+` button (zone 3)**: opens Phase 4a's `WorkspaceCreateDialog`. On success, `onCreated` triggers `refreshWorkspacesAtom` and `selectWorkspaceAtom(newId)` — new workspace appears at end and becomes active.

**Dividers**: between zones 1↔2 and 2↔3, render `<div className="w-px h-5 bg-border/40 mx-1" />`.

### 4.2 `WorkspaceHeader`

**File**: `ui/src/components/workspace/WorkspaceHeader.tsx` (new)

Renders at top of LeftSidebar in **Agent mode only**, between `ModeSwitcher` and the `新会话` button.

```tsx
<div className="group flex items-center gap-2 px-3 py-2 mx-3 mt-1 rounded-md
                hover:bg-foreground/[0.03] transition-colors">
  <span className="text-base leading-none">{active.icon}</span>
  <div className="flex-1 min-w-0">
    <div className="text-[13px] font-semibold truncate">{active.name}</div>
    {active.path && (
      <div className="text-[10px] text-muted-foreground/70 truncate font-mono"
           title={active.path}>
        {active.path.replace(homeDir, '~')}
      </div>
    )}
  </div>
  {canMutate && (
    <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100
                    transition-opacity flex-shrink-0">
      <button onClick={startRename} title="重命名"><Pencil className="size-3.5" /></button>
      <button onClick={openDeleteDialog} title="删除工作区"><Trash2 className="size-3.5" /></button>
    </div>
  )}
</div>
```

**Rename**: clicking ✏ swaps name span for an `<input>`, autofocus + select all. Enter commits via `updateWorkspaceAtom`. Esc cancels. Reuses Phase 2's pattern.

**Delete**: clicking 🗑 opens existing `AlertDialog` confirmation. Confirm calls `deleteWorkspace` IPC (Phase 1). After delete: Phase 1 backend re-homes orphans to 'default'; frontend then `selectWorkspaceAtom('default')` + `refreshWorkspacesAtom`.

**`canMutate = active.id !== 'default'`**: same Phase 2 rule. Default workspace shows read-only header; tooltip on disabled buttons explains "默认工作区不可删除" (only matters if hidden buttons accidentally become visible).

**Path display**: `path.replace(homeDir, '~')` shows `~/Documents/workground/2222`. Long paths truncate with `title` for full path on hover. Path row omitted when `active.path === null`.

### 4.3 `WorkspaceRail` rewrite

**File**: `ui/src/components/workspace/WorkspaceRail.tsx` (rewrite, keep filename)

Renders only the active workspace's sessions as a flat list. Deletes per-workspace iteration; deletes the "+ 新建工作区" button (moved to switcher bar).

```tsx
export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
}: WorkspaceRailProps): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const workspaceSessions = useAtomValue(workspaceSessionsAtom)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const [moveTargetSessionId, setMoveTargetSessionId] = React.useState<string | null>(null)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const moveTargetSession = moveTargetSessionId
    ? agentSessions.find((s) => s.id === moveTargetSessionId) : null

  const sessions = activeWorkspaceId
    ? (workspaceSessions[activeWorkspaceId] ?? []) : []

  return (
    <>
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {sessions.length === 0 && (
          <p className="text-[11px] text-muted-foreground px-2 py-3 italic">
            尚无会话。点击上方"新会话"开始。
          </p>
        )}
        {sessions.map((s) => (
          <SessionItem
            key={s.id}
            id={s.id}
            title={s.title}
            titleEmoji={s.titleEmoji}
            titlePending={s.titlePending}
            isActive={activeSessionId === s.id}
            running={indicatorMap.get(s.id) === 'running'}
            onClick={() => onSelectSession(s.id)}
            onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            onMove={(sid) => setMoveTargetSessionId(sid)}
          />
        ))}
      </div>
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={workspaces.map((w) => ({
            id: w.id, name: w.name, icon: w.icon, path: w.path,
            createdAt: Date.parse(w.createdAt) || Date.now(),
            updatedAt: Date.parse(w.updatedAt) || Date.now(),
          }))}
          onMoved={() => setMoveTargetSessionId(null)}
        />
      )}
    </>
  )
}
```

**Deleted**:
- `<WorkspaceGroup>` per-workspace iteration
- Workspace drag-reorder state + handlers (moved to `WorkspaceSwitcherBar`)
- "+ 新建工作区" button (moved to switcher bar's `+`)

**`WorkspaceGroup.tsx` removed entirely**. Test file `WorkspaceGroup.test.tsx` also removed.

### 4.4 `TabBarWorkspaceChip` downgrade

**File**: `ui/src/components/tabs/TabBarWorkspaceChip.tsx` (modify)

Strip the Radix DropdownMenu wrapper. The chip becomes a passive label:

```tsx
export function TabBarWorkspaceChip(): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const active = workspaces.find((w) => w.id === activeId)
  if (!active) return null

  return (
    <div
      className="titlebar-no-drag flex items-center gap-1 px-2 py-1 rounded-md
                 text-[12px] text-foreground/70 shrink-0"
      title={`工作区: ${active.name}`}
    >
      <span className="leading-none text-[13px]">{active.icon}</span>
      <span className="font-medium">{truncateName(active.name)}</span>
    </div>
  )
}
```

No `<button>`, no `<DropdownMenu>`, no `<ChevronDown>`. Pure label. The `WorkspaceCreateDialog` import + `createOpen` state are removed.

`TabBarWorkspaceChip.test.tsx` updated: drop the dropdown-open / click-item / create-dialog tests; keep the render + truncation + null-active-state tests.

### 4.5 Per-workspace right-panel tab memory

**New atom** in `ui/src/atoms/agent-atoms.ts`:

```ts
import type { ActiveTab } from '@/components/app-shell/RightSidePanel'

export const workspaceActiveRightPanelTabMapAtom =
  atom<Map<string, ActiveTab>>(new Map())
```

**`ActiveTab` type** must be exported from `RightSidePanel.tsx`:
```ts
export type ActiveTab = 'files' | 'teams' | 'plan' | 'trajectory' | 'browser'
```

**`RightSidePanel` changes**:

Replace `const [activeTab, setActiveTab] = React.useState<ActiveTab>('files')` with:

```ts
const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
const tabMap = useAtomValue(workspaceActiveRightPanelTabMapAtom)
const setTabMap = useSetAtom(workspaceActiveRightPanelTabMapAtom)

const activeTab: ActiveTab = activeWorkspaceId
  ? (tabMap.get(activeWorkspaceId) ?? 'files') : 'files'

const setActiveTab = React.useCallback((tab: ActiveTab) => {
  if (!activeWorkspaceId) return
  setTabMap((prev) => {
    const next = new Map(prev)
    next.set(activeWorkspaceId, tab)
    return next
  })
}, [activeWorkspaceId, setTabMap])
```

Update the `plan:updated` listener to route through the map:

```ts
listen<PlanUpdatedPayload>('plan:updated', ({ payload }) => {
  setActivePlan({ filename: payload.filename, content: payload.content })
  if (activeWorkspaceId) {
    setTabMap((prev) => {
      const next = new Map(prev)
      next.set(activeWorkspaceId, 'plan')
      return next
    })
  }
})
```

Plan-updated events only auto-switch the tab for the workspace currently active (which owns the agent firing the event).

### 4.6 Drag-reorder semantics for switcher bar

Mirrors Phase 2's tree drag-reorder but rotated to horizontal:

```tsx
const handleDragOver = (e: React.DragEvent, targetId: string): void => {
  e.preventDefault()
  e.dataTransfer.dropEffect = 'move'
  if (!dragId || dragId === targetId) {
    setDropIndicator(null)
    return
  }
  const rect = e.currentTarget.getBoundingClientRect()
  const ratio = (e.clientX - rect.left) / rect.width     // ← horizontal
  const position: 'before' | 'after' = ratio < 0.5 ? 'before' : 'after'
  setDropIndicator({ id: targetId, position })
}

const handleDrop = async (e: React.DragEvent, targetId: string): Promise<void> => {
  e.preventDefault()
  e.stopPropagation()
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const ratio = (e.clientX - rect.left) / rect.width
  const position: 'before' | 'after' = ratio < 0.5 ? 'before' : 'after'
  const sourceId = dragId ?? e.dataTransfer.getData('text/plain') ?? ''
  setDragId(null); setDropIndicator(null)
  if (!sourceId || sourceId === targetId) return
  const fromIdx = workspaces.findIndex((w) => w.id === sourceId)
  const toIdx = workspaces.findIndex((w) => w.id === targetId)
  if (fromIdx === -1 || toIdx === -1) return
  const reordered = [...workspaces]
  const [moved] = reordered.splice(fromIdx, 1)
  const adjustedToIdx = fromIdx < toIdx ? toIdx - 1 : toIdx
  const insertIdx = position === 'after' ? adjustedToIdx + 1 : adjustedToIdx
  reordered.splice(insertIdx, 0, moved!)
  try { await reorderWorkspaces(reordered.map((w) => w.id)) }
  catch (err) { console.error('[workspace-dnd] reorder failed', err) }
}
```

Drop indicator: 2px vertical `bg-primary rounded-full` line, absolutely positioned on the receiving icon at `left` or `right` edge per `position`.

**Default workspace** IS draggable (sort_order can change, only identity is protected).

**Drop targets**: only workspace icons and dots. Drop on automation or `+` button = no-op (those zones don't register drop handlers).

### 4.7 Running-session indicator

Compute `runningWorkspaceIds` in the switcher bar:

```tsx
const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
const agentSessions = useAtomValue(agentSessionsAtom)
const runningWorkspaceIds = React.useMemo(() => {
  const set = new Set<string>()
  for (const s of agentSessions) {
    if (indicatorMap.get(s.id) === 'running' && s.workspaceId) {
      set.add(s.workspaceId)
    }
  }
  return set
}, [agentSessions, indicatorMap])
```

Render in `WorkspaceIcon`:
```tsx
{running && (
  <span className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full
                   bg-primary animate-pulse shadow-[0_0_4px_hsl(var(--primary))]"
        aria-label="该工作区有任务执行中" />
)}
```

Render in `WorkspaceDot` (smaller, no glow due to density):
```tsx
{running && (
  <span className="absolute -top-px -right-px size-1 rounded-full
                   bg-primary animate-pulse" />
)}
```

### 4.8 LeftSidebar integration

**File**: `ui/src/components/app-shell/LeftSidebar.tsx` (modify)

Final layout (Agent mode, expanded):

```
┌─ LeftSidebar ─────────────────┐
│ [30px drag bar]                │
│ ModeSwitcher                   │
│ ─────                          │
│ <WorkspaceHeader />            │  ← NEW (Agent mode only)
│ ─────                          │
│ [+ 新会话]  [🔍]                │
│ ─────                          │
│ <WorkspaceRail />              │  ← rewritten — active workspace only
│ (flex-1, scrollable)           │
│ ─────                          │
│ <WorkspaceSwitcherBar />       │  ← NEW (Agent mode only)
│ ─────                          │
│ [👤 User]  [⚙ Settings]        │  ← stays at bottom (global)
└────────────────────────────────┘
```

Chat mode: no `WorkspaceHeader`, no `WorkspaceSwitcherBar`. The workspace concepts don't apply in chat mode (sessions are workspace-scoped, conversations aren't).

**Sidebar collapse removal**: the existing collapsed-sidebar branch (`LeftSidebar.tsx:622-664`) is deleted entirely. Specifically:

- `sidebarCollapsedAtom` in `@/atoms/tab-atoms` either deleted or kept as a permanent `false` (deletion preferred — no consumers remain after this change).
- `toggle-sidebar` definition removed from `SHORTCUT_DEFINITIONS` (`shortcut-defaults.ts`). `Cmd+B` becomes unused; users who relied on it can be informed via release notes.
- The corresponding `toggle-sidebar` handler in `GlobalShortcuts.tsx` deleted.
- The collapse button in the sidebar UI (`PanelLeftClose` icon) is removed.
- The 48-px collapsed branch of `LeftSidebar.tsx` is removed; the component returns the expanded layout unconditionally.

This simplifies LeftSidebar from ~999 lines to ~700 lines net (after factoring in the new `WorkspaceHeader` + `WorkspaceSwitcherBar` mounts).

**TabBarWorkspaceChip role**: with sidebar always expanded, the chip becomes purely supplementary (the WorkspaceHeader is the primary identity anchor). Still useful as a context cue in the TabBar chrome. Kept as the passive label per Goal #4.

## 5. Persistence

**No new persistence**:
- `workspaceActiveRightPanelTabMapAtom` is in-memory only. App restart resets all workspaces to Files tab.
- `workspacesAtom` ordering persists via Phase 2's `sort_order` column (no change).
- Rename + delete persist via Phase 1/2's IPCs (no change).

**No new migrations**, no new IPCs.

## 6. Error Handling

- **Active workspace deleted while header visible**: backend Phase 1 helper re-homes orphan sessions to 'default'. Frontend `selectWorkspaceAtom('default')` after delete handles the active-workspace transition.
- **Workspace list refetch fails**: handled by existing Phase 2 atoms (rollback to previous state). Toast surfaces in `WorkspaceCreateDialog` already.
- **Drag-reorder failure**: existing `reorderWorkspacesAtom` (Phase 2 + Phase 3 fix) reverts on backend failure. No new logic needed.
- **Running indicator with stale session data**: `runningWorkspaceIds` recomputes via React.useMemo on every `agentSessions` or `indicatorMap` change. Stale state self-heals within one render cycle.
- **`workspaceId` undefined on a session**: filtered out by `if (... && s.workspaceId)` in `runningWorkspaceIds`. Such sessions are also invisible in the tree (no active workspace = nothing to render); only the global `agentSessionsAtom` knows about them. They get re-homed by Phase 1's orphan-healing on app start.

## 7. Testing

**Vitest** (UI tests, jsdom):

| File | Cases |
|---|---|
| `WorkspaceSwitcherBar.test.tsx` | renders all icons for ≤5; collapses to dots for >5 with only active full; click icon → setActiveWorkspaceId; tooltip renders pill-style chips (workspace name on left, `⌘` + digit pills on right) for first 9; tooltip omits shortcut pills for 10+; running indicator appears when any session in workspace is running; `+` opens CreateDialog; automation button opens slide-over; horizontal drag-reorder calls reorderWorkspaces |
| `WorkspaceHeader.test.tsx` | renders active workspace name + emoji + truncated path; ✏ inline rename → Enter commits via updateWorkspace, Esc cancels; 🗑 opens AlertDialog → confirm calls deleteWorkspace + selectWorkspaceAtom('default'); rename + delete buttons absent when active = 'default' |
| `WorkspaceRail.test.tsx` (replace existing) | renders ONLY active workspace's sessions; empty state shows hint; clicking session calls onSelectSession; three-dot menu → 移动到... opens MoveSessionDialog |
| `RightSidePanel.test.tsx` (new) | active tab follows workspaceActiveRightPanelTabMapAtom per workspace; switching workspace restores its previous tab; plan:updated sets 'plan' only for active workspace |
| `TabBarWorkspaceChip.test.tsx` (update) | renders passive label only; no clickable dropdown; tests for dropdown / dialog dropped |

Total: ~20 new/updated cases. Deleted: ~5 cases from `WorkspaceGroup.test.tsx`.

**Manual smoke checklist** (recorded in PR description):

1. **Switcher bar render**: bottom shows `[🤖] | [📁 📁 📁] | [+]` with dividers.
2. **≤5 vs >5 collapse**: create workspaces; at 5 all full, at 6 non-active collapse to dots.
3. **Tooltip pill style**: hover any icon/dot → tooltip shows workspace name on left + `⌘` pill + digit pill on right (matching ARC's design) for first 9; 10+ shows only name + emoji.
4. **Click switch**: click icon/dot → workspace switches; sessions tree refreshes; right panel restores last tab for that workspace.
5. **Per-workspace tab memory**: switch to A, select Plan tab; switch to B; switch back to A → Plan tab restored.
6. **Workspace header**: top of sidebar shows active workspace name + path; hover ✏ → inline rename works; 🗑 → confirm → workspace deletes, sessions re-homed.
7. **Default protection**: switch to default → no ✏/🗑 buttons.
8. **Running indicator**: start agent in A → switch to B → A's icon shows pulse dot.
9. **Drag-reorder**: drag icon → drop-line indicator → release → order persists.
10. **Create new**: click `+` → CreateDialog → fill name → Create → new workspace appears at end, active.
11. **User + Settings row**: stays below switcher bar.
12. **TabBar chip**: passive label, no dropdown.
13. **⌘1..9 regression**: Phase 4a shortcuts still work.
14. **No collapse**: confirm no collapse button visible in sidebar header area; pressing `Cmd+B` does nothing (no longer mapped).

## 8. PR Shape (bisectable commits)

| # | Commit | LOC est |
|---|---|---|
| 1 | `feat(atoms): workspaceActiveRightPanelTabMapAtom + RightSidePanel per-workspace tab memory` | ~120 |
| 2 | `feat(workspace): WorkspaceHeader — top-of-sidebar name + rename/delete` | ~180 |
| 3 | `feat(workspace): WorkspaceSwitcherBar — icons / dots / drag-reorder / running indicator` | ~400 |
| 4 | `refactor(workspace): WorkspaceRail renders only active workspace; delete WorkspaceGroup` | ~250 |
| 5 | `refactor(layout): TabBarWorkspaceChip → passive label + LeftSidebar layout + remove sidebar collapse` | ~250 |

Total: ~1200 LOC additions + ~300 LOC deletions = net +900 LOC. Build green at every commit:

- Commit 1: new atom + RightSidePanel — existing UI untouched.
- Commit 2: WorkspaceHeader renders ABOVE the existing tree (cosmetic redundancy until commit 4).
- Commit 3: switcher bar renders ABOVE the existing "+ 新建工作区" button (cosmetic redundancy).
- Commit 4: tree collapses to active-only; old WorkspaceGroup + "+ 新建工作区" deleted.
- Commit 5: TabBarWorkspaceChip downgrade + LeftSidebar layout polish + sidebar-collapse removal (`sidebarCollapsedAtom` / `Cmd+B` shortcut / collapsed-mode render branch).

Commit 4 is the load-bearing user-visible change. Commits 1-3 are infrastructure; commit 5 is cleanup.

## 9. Open Questions / Risks

- **`WorkspaceGroup.tsx` deletion**: confirmed only `WorkspaceRail` and tests consume it. Removing won't cascade.
- **Drag visual feedback on 6px dots**: the 2px drop-line is taller than the dot but renders cleanly in the ~6px gap between dots. Mentally verified; will polish during implementation if it looks odd.
- **Reactivity load**: `runningWorkspaceIds` recomputes on every `agentSessionsAtom` or `indicatorMap` change. With 50 sessions across workspaces and frequent tool firing, O(n) Set build per indicator change. Acceptable; no memoization needed in Phase 4b.
- **Sidebar collapse removal risk**: anyone with `Cmd+B` muscle memory loses the shortcut. Mitigated via release notes. If user feedback demands it back, the chord can be remapped to something else (e.g. toggle right panel) without restoring the collapse mode.
- **Theme compatibility**: 11 themes (Phase 1-3). The switcher bar uses theme tokens (`bg-primary/10`, `bg-foreground/[0.06]`, `border-border/40`) — should survive all themes. Will verify warm-paper, qingye, forest-dark during manual smoke.

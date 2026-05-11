# Per-Workspace Tab Memory — Design

> **Phase 5 of the workspace remediation series.** Phase 4b shipped the
> ARC-style switcher; this fixes the resulting bug where tabs from every
> workspace are rendered into the active workspace's TabBar.

## 1. Problem

`tabsAtom` (`ui/src/atoms/tab-atoms.ts:37`) and `activeTabIdAtom`
(`:38`) are **global flat state**. Opening a session in workspace A and
then switching to workspace B leaves A's tabs visible in B's TabBar.

User-visible symptom (2026-05-11 bug report screenshot): five tabs
spanning three workspaces are all stuck in the active workspace's bar,
with no way to tell which tab belongs to which workspace.

The right-side panel was already remediated in Phase 4b via
`workspaceActiveRightPanelTabMapAtom` (`ui/src/atoms/agent-atoms.ts:281`).
This spec applies the same pattern to the top TabBar.

## 2. Goal

Each workspace owns its own list of open tabs and its own active-tab
selection. Switching workspaces hides the other workspace's tabs (they
are **remembered**, not destroyed — coming back restores them).

## 3. Non-Goals

- Persistence across app restarts — out of scope. The atom is in-memory.
  (Tabs were never persisted anyway. We're not regressing.)
- Backend changes — pure frontend refactor, no Rust, no migrations.
- Cross-workspace tab move (drag tab from one workspace into another) —
  out of scope; revisit if users ask.
- Tab content state (scroll position, minimap cache, etc.) — already
  keyed by tab id, no change needed.

## 3.5. In Scope — Right panel must follow workspace switch

The right-side panel (`ui/src/components/app-shell/RightSidePanel.tsx`)
is gated on `currentAgentSessionIdAtom` (line 112). Today, this atom
is only written by the TabBar's `onClick` handler
(`ui/src/components/tabs/TabBar.tsx:71`) — there's no listener that
re-points it when the active workspace changes.

After this PR, switching workspace will flip `activeTabIdAtom` via
the per-workspace map. But the right panel won't refresh unless
`currentAgentSessionIdAtom` (and the parallel
`currentConversationIdAtom` for chat tabs) also flips. Two options:

- **Option 1 — Effect-driven sync (recommended):** Add a small
  `<TabSessionSyncer />` mounted in `AppShell` that subscribes to
  `activeTabAtom` and writes the matching session/conversation atom
  whenever the active tab changes. TabBar's click handler keeps its
  writes (defensive duplicates — harmless because the new value
  matches what the syncer would compute).
- **Option 2 — Derive `currentAgentSessionIdAtom`:** Replace the
  primitive atom with a derived atom that reads
  `activeTabAtom?.type === 'agent' ? activeTabAtom.sessionId : null`.
  Cleaner architecturally but `currentAgentSessionIdAtom` has 80+
  consumers (most via subscriptions to derived atoms) so the blast
  radius is wide.

**Pick Option 1** — one new ~40-line file + AppShell mount. Keeps the
existing atom shape, isolates the sync logic, easy to test.

If the active workspace has no tabs (fresh workspace), the syncer
sets both atoms to `null` and the right panel hides (its existing
empty-state path).

## 4. Approach — Option A (tag + derive)

Tag each `TabItem` with a `workspaceId`. Keep `tabsAtom` as a single
flat array (the pool). Render the TabBar / MainArea from a derived
`visibleTabsAtom` that filters by `activeWorkspaceIdAtom`. Move the
active-tab pointer into a per-workspace map, the same shape as
`workspaceActiveRightPanelTabMapAtom`.

```ts
interface TabItem {
  id: string
  type: TabType
  sessionId: string
  title: string
  workspaceId: string   // NEW — required, no default
}

// Pool of every open tab across all workspaces.
export const tabsAtom = atom<TabItem[]>([])

// Per-workspace active-tab pointer.
export const workspaceActiveTabIdMapAtom =
  atom<Map<string, string | null>>(new Map())

// Derived — what the active workspace currently sees.
export const visibleTabsAtom = atom((get) => {
  const wsId = get(activeWorkspaceIdAtom)
  if (!wsId) return []
  return get(tabsAtom).filter((t) => t.workspaceId === wsId)
})

// Read-write proxy that reads/writes the active workspace's slot.
export const activeTabIdAtom = atom(
  (get) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return null
    return get(workspaceActiveTabIdMapAtom).get(wsId) ?? null
  },
  (get, set, next: string | null) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return
    const m = new Map(get(workspaceActiveTabIdMapAtom))
    if (next === null) m.delete(wsId)
    else m.set(wsId, next)
    set(workspaceActiveTabIdMapAtom, m)
  },
)
```

### Why this shape

- **Reuses the Phase 4b pattern** verbatim (Map keyed by workspace id;
  derived getter for the active slot). Same mental model, same testing
  approach.
- **Preserves the existing read API** — `activeTabIdAtom` stays a
  read-write atom that returns `string | null`, so the 13+ consumers
  (TabBar, MainArea, TabSwitcher, etc.) don't change.
- **`openTab` / `closeTab` / `reorderTabs` stay pure functions** — they
  just gain a `workspaceId` parameter passed in by the caller from
  `activeWorkspaceIdAtom`.
- **Tabs are remembered** — switching workspace away and back restores
  the previous tabs and the previously-active tab.

### Edge cases

- **`openTab` called when no workspace is active** (race during initial
  boot): drop the call. The session can be opened later. This already
  happens — there's a guard in `useOpenSession`.
- **Tab whose session was moved to a different workspace** (Phase 2 has
  a MoveSessionDialog): the tab keeps its original `workspaceId` —
  it's a UX choice, not a data choice. Power users moving a session
  expect the tab to remain visible in its current workspace until
  closed. Documenting this explicitly so the implementer doesn't
  "fix" it.
- **Workspace deletion** (Phase 2 re-homes orphan sessions to 'default'):
  after the deletion, tabs whose `workspaceId` no longer exists become
  orphans in the pool. Add a cleanup pass triggered by
  `workspacesAtom` change — drop orphan tabs and clear their entries
  in `workspaceActiveTabIdMapAtom`. ~10 LOC.
- **`agentRecommendBanner` / `MigrateToAgentButton` migrate a chat
  session to an agent session**: the new tab should inherit the
  current workspace, not the source tab's workspace. (In practice
  they're identical, but the implementation reads
  `activeWorkspaceIdAtom` at the moment of opening.)

## 5. Why not Option B (per-workspace Map<workspaceId, Tab[]>)

Considered but rejected:
- 53 usage sites across 16 files would all need to thread a
  `workspaceId` through. ~600+ LOC change vs. ~350 for Option A.
- The data is conceptually a flat pool — tabs aren't owned by
  workspaces in storage, only in display semantics. Pretending
  otherwise adds Map-of-arrays bookkeeping (entry creation, deletion,
  empty-slot management) that buys nothing the filter doesn't.
- The pure functions (`openTab`, `closeTab`, `reorderTabs`) become
  awkward — they'd take a `Map`, return a `Map`, and lose their
  intuitive list-in-list-out shape.

## 6. File-by-file changes

```
ui/src/atoms/tab-atoms.ts
  - TabItem gains workspaceId: string (required)
  - tabsAtom unchanged (still TabItem[])
  - activeTabIdAtom rewritten as derived read-write atom
  - NEW: workspaceActiveTabIdMapAtom (Map<string, string|null>)
  - NEW: visibleTabsAtom (derived filter)
  - openTab(tabs, item) signature gains workspaceId in item
  - closeTab signature unchanged (works by tab id, no workspaceId
    needed)

ui/src/components/tabs/TabBar.tsx
  - Read visibleTabsAtom instead of tabsAtom

ui/src/components/tabs/MainArea.tsx
  - Read visibleTabsAtom instead of tabsAtom for layout decisions

ui/src/components/tabs/TabContent.tsx
  - Read visibleTabsAtom (only renders the active workspace's content
    region anyway, but keeps the indexing consistent)

ui/src/components/tabs/TabSwitcher.tsx
  - Read visibleTabsAtom

ui/src/components/tabs/TabCloseConfirmDialog.tsx
  - Read visibleTabsAtom

ui/src/components/app-shell/AppShell.tsx
  - openTab(...) callers read activeWorkspaceIdAtom and pass it in

ui/src/components/app-shell/ModeSwitcher.tsx
  - Read visibleTabsAtom for the "tab count" badge

ui/src/components/chat/AgentRecommendBanner.tsx
  - openTab caller adds workspaceId

ui/src/components/chat/MigrateToAgentButton.tsx
  - openTab caller adds workspaceId

ui/src/hooks/useOpenSession.ts
  - openTab caller adds workspaceId

ui/src/hooks/useCloseTab.ts
  - No changes (works by tab id)

ui/src/atoms/working-atoms.ts
  - Read visibleTabsAtom (the "has open tabs" derived check should
    be per-workspace too — if all of A's tabs are closed, the empty
    state should show even if B has tabs)

ui/src/atoms/workspace.ts (or wherever workspacesAtom lives)
  - Orphan cleanup hook on workspace deletion (drop tabs whose
    workspaceId is no longer in the workspaces list)

ui/src/components/app-shell/TabSessionSyncer.tsx  (NEW)
  - Subscribes to activeTabAtom; writes
    currentAgentSessionIdAtom / currentConversationIdAtom to match.
  - Returns null (effect-only component).

ui/src/components/app-shell/AppShell.tsx
  - Mount <TabSessionSyncer /> alongside <App />.
```

## 7. Tests

- `tab-atoms.test.ts` (new):
  - `openTab` creates a tab with the given workspaceId
  - `visibleTabsAtom` filters by active workspace
  - `activeTabIdAtom` reads/writes the right slot of the map
  - Switching workspace flips both visibleTabs and activeTabId
  - Orphan cleanup removes tabs of a deleted workspace
- `TabBar.test.tsx`: rendering A→switch→B shows only B's tabs
- `MainArea.test.tsx`: keeps existing empty-state behavior per
  workspace

## 8. Commit shape (6 bisectable commits)

1. `feat(atoms): tag TabItem with workspaceId + per-workspace
   active-tab map`
2. `refactor(tabs): TabBar/MainArea/TabSwitcher/TabContent read
   visibleTabsAtom`
3. `refactor(tabs): openTab callers pass activeWorkspaceId
   (AppShell + banners + useOpenSession)`
4. `feat(tabs): TabSessionSyncer — right panel follows workspace
   switch by re-pointing currentAgentSessionIdAtom`
5. `feat(tabs): orphan cleanup on workspace deletion`
6. `test(tabs): per-workspace tab memory + workspace-switch flip
   + right-panel sync`

Each commit keeps the build green and runtime working (with the
caveat that until step 3 lands, new tabs default to a fallback —
which is exactly the legacy behavior). Bisect-friendly.

## 9. Risk register

- **The fallback during the migration window** (commits 1+2 land but
  3 hasn't): tabs created via legacy callers would lack a
  workspaceId. Solution: make `workspaceId` **required** in TabItem
  from commit 1, but provide a `DEFAULT_WORKSPACE_ID = 'default'` for
  the openTab function default. Eliminates the gap.
- **Test files referencing `tabsAtom` directly** (5 spots per grep):
  update them to provide workspaceId when seeding.
- **localStorage**: tabsAtom isn't persisted today, so no migration
  needed. Document the in-memory behavior in the atom JSDoc so
  future-us doesn't add `atomWithStorage` without thinking about
  workspaceId.

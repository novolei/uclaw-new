# Per-Workspace Tab Memory — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make tabs per-workspace so each workspace owns its own TabBar
list and active-tab pointer, and ensure the right-side panel
auto-updates when switching workspaces.

**Architecture:** Tag every `TabItem` with a `workspaceId`. Keep
`tabsAtom` as a single flat pool. UI reads a derived `visibleTabsAtom`
filtered by `activeWorkspaceIdAtom`. Active-tab pointer moves into a
`workspaceActiveTabIdMapAtom` (same shape as Phase 4b's
`workspaceActiveRightPanelTabMapAtom`). A new `TabSessionSyncer`
component watches `activeTabAtom` and writes
`currentAgentSessionIdAtom` / `currentConversationIdAtom` so the right
panel + body content follow workspace switches.

**Tech Stack:** React 18, Jotai, TypeScript, Vitest + React Testing
Library.

**Spec reference:** `docs/superpowers/specs/2026-05-11-per-workspace-tabs-design.md`

---

## File Structure Overview

```
ui/src/atoms/tab-atoms.ts             (MODIFIED — atoms + types)
ui/src/atoms/agent-atoms.ts           (NO CHANGE — pattern reference)
ui/src/atoms/workspace.ts             (MODIFIED — orphan cleanup on delete)
ui/src/atoms/working-atoms.ts         (MODIFIED — read visibleTabsAtom)
ui/src/components/tabs/TabBar.tsx               (MODIFIED — read visibleTabsAtom)
ui/src/components/tabs/MainArea.tsx             (MODIFIED — read visibleTabsAtom)
ui/src/components/tabs/TabContent.tsx           (MODIFIED — read visibleTabsAtom)
ui/src/components/tabs/TabSwitcher.tsx          (MODIFIED — read visibleTabsAtom)
ui/src/components/tabs/TabCloseConfirmDialog.tsx (MODIFIED — read visibleTabsAtom)
ui/src/components/app-shell/ModeSwitcher.tsx    (MODIFIED — read visibleTabsAtom)
ui/src/components/app-shell/AppShell.tsx        (MODIFIED — openTab caller + mount syncer)
ui/src/components/app-shell/TabSessionSyncer.tsx (NEW — effect-only sync component)
ui/src/components/chat/AgentRecommendBanner.tsx (MODIFIED — openTab caller)
ui/src/components/chat/MigrateToAgentButton.tsx (MODIFIED — openTab caller)
ui/src/hooks/useOpenSession.ts                  (MODIFIED — openTab caller)
ui/src/atoms/tab-atoms.test.ts                  (NEW — atom unit tests)
ui/src/components/tabs/TabBar.test.tsx          (NEW — render flip test)
ui/src/components/app-shell/TabSessionSyncer.test.tsx (NEW — sync effect test)
```

---

## Task 1: Tag `TabItem` with `workspaceId` + per-workspace active-tab map

**Files:**
- Modify: `ui/src/atoms/tab-atoms.ts:23-145`

This task adds the new atom shape WITHOUT touching consumers yet. The
build stays green because `activeTabIdAtom` keeps the same read/write
signature (string | null), it's just derived from the map now. New
tabs default `workspaceId` to `'default'` when no workspace is active —
matches legacy global-tabs behavior during the transition window.

- [ ] **Step 1: Write the failing test**

Create `ui/src/atoms/tab-atoms.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  tabsAtom,
  activeTabIdAtom,
  visibleTabsAtom,
  workspaceActiveTabIdMapAtom,
  openTab,
  closeTab,
  type TabItem,
} from './tab-atoms'
import { activeWorkspaceIdAtom } from './workspace'

function tab(id: string, workspaceId: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId }
}

describe('tab-atoms — per-workspace memory', () => {
  it('visibleTabsAtom filters tabs by active workspace', () => {
    const store = createStore()
    store.set(tabsAtom, [tab('a1', 'ws-1'), tab('a2', 'ws-1'), tab('b1', 'ws-2')])
    store.set(activeWorkspaceIdAtom, 'ws-1')
    expect(store.get(visibleTabsAtom).map((t) => t.id)).toEqual(['a1', 'a2'])
    store.set(activeWorkspaceIdAtom, 'ws-2')
    expect(store.get(visibleTabsAtom).map((t) => t.id)).toEqual(['b1'])
  })

  it('activeTabIdAtom reads/writes the slot for the active workspace', () => {
    const store = createStore()
    store.set(activeWorkspaceIdAtom, 'ws-1')
    store.set(activeTabIdAtom, 'a1')
    store.set(activeWorkspaceIdAtom, 'ws-2')
    expect(store.get(activeTabIdAtom)).toBeNull()
    store.set(activeTabIdAtom, 'b1')
    store.set(activeWorkspaceIdAtom, 'ws-1')
    expect(store.get(activeTabIdAtom)).toBe('a1')
  })

  it('openTab carries the supplied workspaceId onto the new tab', () => {
    const result = openTab([], {
      type: 'agent', sessionId: 's1', title: 't', workspaceId: 'ws-1',
    })
    expect(result.tabs[0]?.workspaceId).toBe('ws-1')
    expect(result.activeTabId).toBe('s1')
  })

  it('closeTab works by tab id (no workspaceId needed)', () => {
    const tabs = [tab('a1', 'ws-1'), tab('a2', 'ws-1')]
    const result = closeTab(tabs, 'a1', 'a1')
    expect(result.tabs.map((t) => t.id)).toEqual(['a2'])
    expect(result.activeTabId).toBe('a2')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run tab-atoms.test.ts`
Expected: FAIL — `visibleTabsAtom`, `workspaceActiveTabIdMapAtom` not
exported; `TabItem.workspaceId` missing.

- [ ] **Step 3: Rewrite `ui/src/atoms/tab-atoms.ts`**

Replace the existing file with:

```ts
/**
 * Tab Atoms — 标签页状态管理
 *
 * Tabs are tagged with a workspaceId. The flat `tabsAtom` is the pool;
 * `visibleTabsAtom` is the derived per-workspace filter that the
 * TabBar / MainArea render from. The active-tab pointer is held in
 * `workspaceActiveTabIdMapAtom` (one slot per workspace) and exposed
 * via the read-write `activeTabIdAtom` that reads/writes the slot for
 * the currently-active workspace.
 *
 * In-memory only — no persistence across app restarts (today's
 * behavior; not regressing).
 */

import { atom } from 'jotai'
import { activeWorkspaceIdAtom } from './workspace'
import {
  streamingConversationIdsAtom,
} from './chat-atoms'
import {
  agentRunningSessionIdsAtom,
  agentSessionIndicatorMapAtom,
  workingDoneSessionIdsAtom,
} from './agent-atoms'
import type { SessionIndicatorStatus } from './agent-atoms'

// ===== 类型定义 =====

export type TabType = 'chat' | 'agent' | 'browser'

export interface TabItem {
  id: string
  type: TabType
  sessionId: string
  title: string
  workspaceId: string
}

export interface PersistedTabState {
  tabs: TabItem[]
  activeTabId: string | null
}

// ===== 核心 Atoms =====

/** Pool of every open tab across all workspaces. */
export const tabsAtom = atom<TabItem[]>([])

/** Per-workspace active-tab pointer. Map<workspaceId, tabId | null>. */
export const workspaceActiveTabIdMapAtom =
  atom<Map<string, string | null>>(new Map())

/** Tabs visible in the currently-active workspace. Derived filter. */
export const visibleTabsAtom = atom<TabItem[]>((get) => {
  const wsId = get(activeWorkspaceIdAtom)
  if (!wsId) return []
  return get(tabsAtom).filter((t) => t.workspaceId === wsId)
})

/**
 * Read/write the active workspace's active-tab id. Reads return the
 * slot for the currently-active workspace; writes update that slot
 * (and only that slot) in workspaceActiveTabIdMapAtom.
 *
 * Returns null when no workspace is active or the workspace has no
 * active tab on record.
 */
export const activeTabIdAtom = atom<string | null, [string | null], void>(
  (get) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return null
    return get(workspaceActiveTabIdMapAtom).get(wsId) ?? null
  },
  (get, set, next) => {
    const wsId = get(activeWorkspaceIdAtom)
    if (!wsId) return
    const m = new Map(get(workspaceActiveTabIdMapAtom))
    if (next === null) m.delete(wsId)
    else m.set(wsId, next)
    set(workspaceActiveTabIdMapAtom, m)
  },
)

export const tabMruAtom = atom<string[]>([])

export interface TabMinimapItem {
  id: string
  role: 'user' | 'assistant' | 'status'
  preview: string
  avatar?: string
  model?: string
}
export const tabMinimapCacheAtom = atom<Map<string, TabMinimapItem[]>>(new Map())

// ===== 派生 Atoms =====

export const activeTabAtom = atom<TabItem | null>((get) => {
  const activeId = get(activeTabIdAtom)
  if (!activeId) return null
  return get(tabsAtom).find((t) => t.id === activeId) ?? null
})

export const tabStreamingMapAtom = atom<Map<string, boolean>>((get) => {
  const tabs = get(tabsAtom)
  const chatStreaming = get(streamingConversationIdsAtom)
  const agentRunning = get(agentRunningSessionIdsAtom)
  const map = new Map<string, boolean>()
  for (const tab of tabs) {
    if (tab.type === 'chat') {
      map.set(tab.id, chatStreaming.has(tab.sessionId))
    } else if (tab.type === 'agent') {
      map.set(tab.id, agentRunning.has(tab.sessionId))
    } else if (tab.type === 'browser') {
      map.set(tab.id, false)
    }
  }
  return map
})

export const tabIndicatorMapAtom = atom<Map<string, SessionIndicatorStatus>>((get) => {
  const tabs = get(tabsAtom)
  const chatStreaming = get(streamingConversationIdsAtom)
  const agentIndicator = get(agentSessionIndicatorMapAtom)
  const workingDoneIds = get(workingDoneSessionIdsAtom)
  const map = new Map<string, SessionIndicatorStatus>()
  for (const tab of tabs) {
    if (tab.type === 'chat') {
      map.set(tab.id, chatStreaming.has(tab.sessionId) ? 'running' : 'idle')
    } else if (tab.type === 'agent') {
      const status = agentIndicator.get(tab.sessionId)
        ?? (workingDoneIds.has(tab.sessionId) ? 'completed' : 'idle')
      map.set(tab.id, status)
    } else if (tab.type === 'browser') {
      map.set(tab.id, 'idle')
    }
  }
  return map
})

// ===== 操作函数 =====

export function openTab(
  tabs: TabItem[],
  item: { type: TabType; sessionId: string; title: string; workspaceId: string },
): { tabs: TabItem[]; activeTabId: string } {
  const existingTab = tabs.find(
    (t) => t.sessionId === item.sessionId && t.type === item.type,
  )
  if (existingTab) {
    return { tabs, activeTabId: existingTab.id }
  }
  const newTab: TabItem = {
    id: item.sessionId,
    type: item.type,
    sessionId: item.sessionId,
    title: item.title,
    workspaceId: item.workspaceId,
  }
  return {
    tabs: [...tabs, newTab],
    activeTabId: newTab.id,
  }
}

export function closeTab(
  tabs: TabItem[],
  activeTabId: string | null,
  tabId: string,
): { tabs: TabItem[]; activeTabId: string | null } {
  const tabIndex = tabs.findIndex((t) => t.id === tabId)
  if (tabIndex === -1) return { tabs, activeTabId }
  const newTabs = tabs.filter((t) => t.id !== tabId)
  let newActiveTabId = activeTabId
  if (activeTabId === tabId) {
    if (newTabs.length > 0) {
      const nextIndex = Math.min(tabIndex, newTabs.length - 1)
      newActiveTabId = newTabs[nextIndex]!.id
    } else {
      newActiveTabId = null
    }
  }
  return { tabs: newTabs, activeTabId: newActiveTabId }
}

export function reorderTabs(
  tabs: TabItem[],
  fromIndex: number,
  toIndex: number,
): TabItem[] {
  if (fromIndex === toIndex) return tabs
  const newTabs = [...tabs]
  const [moved] = newTabs.splice(fromIndex, 1)
  newTabs.splice(toIndex, 0, moved!)
  return newTabs
}

export function updateTabTitle(
  tabs: TabItem[],
  sessionId: string,
  title: string,
): TabItem[] {
  return tabs.map((t) =>
    t.sessionId === sessionId ? { ...t, title } : t,
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run tab-atoms.test.ts`
Expected: PASS (4/4)

- [ ] **Step 5: Verify TypeScript compiles across consumers**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -30`
Expected: errors in callers of `openTab(...)` that don't pass `workspaceId`
(`AppShell.tsx`, `AgentRecommendBanner.tsx`, `MigrateToAgentButton.tsx`,
`useOpenSession.ts`). These are fixed in Task 3.

This is **intentional**. The TS errors mark every site that needs
updating. Pin them with a quick console note so Task 3 can verify.

- [ ] **Step 6: Commit**

```bash
git add ui/src/atoms/tab-atoms.ts ui/src/atoms/tab-atoms.test.ts
git commit -m "feat(atoms): tag TabItem with workspaceId + per-workspace active-tab map

tabsAtom stays a flat pool of every open tab across workspaces.
visibleTabsAtom is a derived filter that the TabBar / MainArea will
render from. workspaceActiveTabIdMapAtom holds the active-tab
pointer per workspace; activeTabIdAtom is a read-write proxy that
reads/writes the slot for the currently-active workspace.

openTab now requires workspaceId in its item argument. Consumers
in AppShell / banners / useOpenSession are updated in a follow-up
commit — TypeScript will mark each missing site."
```

---

## Task 2: Switch UI consumers to `visibleTabsAtom`

**Files:**
- Modify: `ui/src/components/tabs/TabBar.tsx:36`
- Modify: `ui/src/components/tabs/MainArea.tsx:17`
- Modify: `ui/src/components/tabs/TabContent.tsx:21`
- Modify: `ui/src/components/tabs/TabSwitcher.tsx:33`
- Modify: `ui/src/components/tabs/TabCloseConfirmDialog.tsx:26`
- Modify: `ui/src/components/app-shell/ModeSwitcher.tsx:33`
- Modify: `ui/src/atoms/working-atoms.ts:32` (the `hasAnyTab`-style
  derived atom should be per-workspace)

These files render the active workspace's tabs only. They keep reading
`activeTabIdAtom` as-is (now per-workspace via the proxy).

**Important:** ONLY change `useAtomValue(tabsAtom)` → `useAtomValue(visibleTabsAtom)`.
Writers of `tabsAtom` (the ones that call `setTabs`) keep writing
`tabsAtom` because they're modifying the global pool. The drag-reorder
logic in `TabBar.tsx` operates on the visible slice — it should
splice/reorder the visible slice and then write back to `tabsAtom`
after merging the unchanged other-workspace tabs back in. See Step 4.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/tabs/TabBar.test.tsx` (or extend existing):

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen } from '@testing-library/react'
import { TabBar } from './TabBar'
import { tabsAtom, workspaceActiveTabIdMapAtom, type TabItem } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom, workspacesAtom } from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({ listSpaces: vi.fn().mockResolvedValue([]) }))

function mk(id: string, ws: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId: ws }
}

describe('TabBar — per-workspace visibility', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('renders only the active workspace\'s tabs', () => {
    const store = createStore()
    store.set(tabsAtom, [mk('a1', 'ws-1'), mk('a2', 'ws-1'), mk('b1', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'a1'], ['ws-2', 'b1']]))
    store.set(workspacesAtom, [
      { id: 'ws-1', name: 'A', icon: 'Folder', path: '/a', attachedDirs: [], sortOrder: 0, createdAt: '', updatedAt: '' },
      { id: 'ws-2', name: 'B', icon: 'Folder', path: '/b', attachedDirs: [], sortOrder: 1, createdAt: '', updatedAt: '' },
    ])
    store.set(activeWorkspaceIdAtom, 'ws-1')
    const { rerender } = render(<Provider store={store}><TabBar /></Provider>)
    expect(screen.getByText('a1')).toBeInTheDocument()
    expect(screen.getByText('a2')).toBeInTheDocument()
    expect(screen.queryByText('b1')).not.toBeInTheDocument()

    store.set(activeWorkspaceIdAtom, 'ws-2')
    rerender(<Provider store={store}><TabBar /></Provider>)
    expect(screen.queryByText('a1')).not.toBeInTheDocument()
    expect(screen.queryByText('a2')).not.toBeInTheDocument()
    expect(screen.getByText('b1')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run TabBar.test.tsx`
Expected: FAIL — currently `tabs` reads the global pool so all three
tabs are rendered regardless of workspace.

- [ ] **Step 3: Update reader sites**

For each file in the **Files** list, change the import to add
`visibleTabsAtom` and swap the read.

Pattern:
```ts
// Before
import { tabsAtom, activeTabIdAtom } from '@/atoms/tab-atoms'
const tabs = useAtomValue(tabsAtom)

// After
import { tabsAtom, visibleTabsAtom, activeTabIdAtom } from '@/atoms/tab-atoms'
const tabs = useAtomValue(visibleTabsAtom)
```

**Specific notes per file:**

- **`TabBar.tsx`**: `tabs` is read in `handleActivate` (line 63),
  `handleDragStart` (line 93), and rendered to `<TabBarInner tabs={tabs}>`.
  Use `visibleTabsAtom` for all three. Drag-reorder writes are addressed
  in Step 4 below.
- **`MainArea.tsx`**: only checks tab count + finds active tab content.
  Switch to `visibleTabsAtom`.
- **`TabContent.tsx`**: maps tabs to rendered content panels — needs
  visible slice.
- **`TabSwitcher.tsx`**: Cmd+K-style switcher. **Decision:** keep this
  as visible-only too (don't let users switch to a hidden tab from
  another workspace). Use `visibleTabsAtom`.
- **`TabCloseConfirmDialog.tsx`**: reads `tabs` to find the one being
  closed by id. Either `tabsAtom` or `visibleTabsAtom` works (the
  tab MUST be visible for the user to have clicked it). Use
  `visibleTabsAtom` for consistency.
- **`ModeSwitcher.tsx`**: read line 33 shows `tabs` count badges.
  Switch to `visibleTabsAtom` so the badge reflects the current
  workspace.
- **`working-atoms.ts:32`**: derived `hasAnyOpenTab` (or similar). It
  reads `tabsAtom`. If the consumer wants "does the active workspace
  have tabs", change to `visibleTabsAtom`. If it wants "does ANY
  workspace have tabs", leave on `tabsAtom`. Read the file to decide;
  most likely the right answer is `visibleTabsAtom`.

- [ ] **Step 4: Fix TabBar drag-reorder write path**

`TabBar.tsx` has drag-reorder logic that does
`setTabs(reorderTabs(tabs, from, to))`. After Task 2, `tabs` is the
visible slice. Writing it back to `tabsAtom` would clobber the
other-workspace tabs.

Fix: after computing the reordered visible slice, splice it back into
the global pool, preserving non-visible tabs' positions among the
visible ones' new positions.

```ts
// In TabBar.tsx — inside the pointerup handler:
const setTabs = useSetAtom(tabsAtom)
const allTabs = useAtomValue(tabsAtom)   // global pool

// On reorder commit:
const reorderedVisible = reorderTabs(tabs, fromIdx, toIdx)
// Re-merge: walk allTabs, replace the visible-slot positions with
// reorderedVisible in order.
const visibleIds = new Set(reorderedVisible.map((t) => t.id))
let visIdx = 0
const merged = allTabs.map((t) =>
  visibleIds.has(t.id) ? reorderedVisible[visIdx++]! : t,
)
setTabs(merged)
```

Add a small unit test in `tab-atoms.test.ts`:

```ts
it('drag-reorder of visible slice does not disturb other workspaces', () => {
  // Simulated reorder helper used by TabBar — keep this test colocated
  // with the atoms even though the helper lives in TabBar.tsx.
  // Setup: ws-1 has [a1, a2, a3], ws-2 has [b1]; reorder a3 to position 0.
  // Expected: ws-1's visible becomes [a3, a1, a2]; ws-2 still has [b1].
  // ... see plan §Task 2 for the merge logic ...
})
```

(Actually keep this test in TabBar.test.tsx since the merge logic
lives there.)

- [ ] **Step 5: Run all tests**

Run: `cd ui && npm test -- --run`
Expected: All tab tests pass; rest of the suite unaffected.

- [ ] **Step 6: Verify TypeScript**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -30`
Expected: openTab call sites still error (Task 3 fixes those). No new
errors introduced by Task 2.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/tabs/TabBar.tsx ui/src/components/tabs/MainArea.tsx \
  ui/src/components/tabs/TabContent.tsx ui/src/components/tabs/TabSwitcher.tsx \
  ui/src/components/tabs/TabCloseConfirmDialog.tsx \
  ui/src/components/app-shell/ModeSwitcher.tsx ui/src/atoms/working-atoms.ts \
  ui/src/components/tabs/TabBar.test.tsx ui/src/atoms/tab-atoms.test.ts
git commit -m "refactor(tabs): TabBar + MainArea + switchers read visibleTabsAtom

Reader sites that render the active workspace's tabs now subscribe
to visibleTabsAtom (per-workspace filter) instead of the global
tabsAtom pool. TabBar's drag-reorder write path splices the
reordered visible slice back into the pool so the other workspaces'
tab order is preserved.

This commit alone does not fix the bug — new tabs created by the
banners and useOpenSession still default to workspaceId='default'
because the openTab callers haven't been updated yet. That ships
in the next commit."
```

---

## Task 3: `openTab` callers pass active workspace id

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx:161,188`
- Modify: `ui/src/components/chat/AgentRecommendBanner.tsx:78`
- Modify: `ui/src/components/chat/MigrateToAgentButton.tsx:73`
- Modify: `ui/src/hooks/useOpenSession.ts:42`

Each caller reads `activeWorkspaceIdAtom` and threads it into the
`openTab(...)` item argument.

- [ ] **Step 1: Update each call site**

For each file, add the import + read + pass:

```ts
// Pattern (for useAtomValue-friendly sites):
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)

// At the call:
const ws = activeWorkspaceId ?? 'default'
const result = openTab(tabs, { type, sessionId, title, workspaceId: ws })
```

For the `store.set(...)` sites in `AgentRecommendBanner` and
`MigrateToAgentButton` (which use `useStore` patterns), read via
`store.get(activeWorkspaceIdAtom)` instead:

```ts
const ws = store.get(activeWorkspaceIdAtom) ?? 'default'
const result = openTab(tabs, { ..., workspaceId: ws })
```

- [ ] **Step 2: Verify TypeScript clean**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: NO errors. All openTab call sites supply workspaceId.

- [ ] **Step 3: Verify tests**

Run: `cd ui && npm test -- --run`
Expected: All pass.

- [ ] **Step 4: Smoke-test manually (optional, for sanity)**

```bash
cd src-tauri && cargo tauri dev
```

In the app:
1. Open a session in workspace A. Tab appears.
2. Switch to workspace B via ⌘2. Tab list is empty.
3. Open a session in B. Tab appears.
4. ⌘1 back to A. Only A's tab is visible.
5. ⌘2 back to B. Only B's tab is visible.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/app-shell/AppShell.tsx \
  ui/src/components/chat/AgentRecommendBanner.tsx \
  ui/src/components/chat/MigrateToAgentButton.tsx \
  ui/src/hooks/useOpenSession.ts
git commit -m "refactor(tabs): openTab callers pass active workspace id

AppShell (deep-link / recent-list opens), AgentRecommendBanner,
MigrateToAgentButton, and useOpenSession now read
activeWorkspaceIdAtom at call time and tag the new tab with that
workspace. Fallback 'default' covers the very-early-boot race where
no workspace is active yet — matches the V16 'default' workspace
that always exists in the DB."
```

---

## Task 4: `TabSessionSyncer` — right panel follows workspace switch

**Files:**
- Create: `ui/src/components/app-shell/TabSessionSyncer.tsx`
- Create: `ui/src/components/app-shell/TabSessionSyncer.test.tsx`
- Modify: `ui/src/components/app-shell/AppShell.tsx` (mount syncer)

When the user switches workspace, `activeTabIdAtom` flips
automatically (because of the per-workspace map proxy). But
`currentAgentSessionIdAtom` / `currentConversationIdAtom` are NOT
flipped — they're only written when a user clicks a tab. Without
this syncer, the right panel keeps showing the previous workspace's
session content.

The syncer is an effect-only component (returns null). It watches
`activeTabAtom` and writes the matching session/conversation atom
when the tab type/sessionId changes.

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/app-shell/TabSessionSyncer.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { TabSessionSyncer } from './TabSessionSyncer'
import {
  tabsAtom, workspaceActiveTabIdMapAtom, type TabItem,
} from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { appModeAtom } from '@/atoms/app-mode'

function mk(id: string, type: 'agent' | 'chat', ws: string): TabItem {
  return { id, type, sessionId: id, title: id, workspaceId: ws }
}

vi.mock('@/lib/tauri-bridge', () => ({}))

describe('TabSessionSyncer', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('rewrites currentAgentSessionIdAtom when workspace switch flips the active tab', () => {
    const store = createStore()
    store.set(tabsAtom, [mk('a1', 'agent', 'ws-1'), mk('b1', 'agent', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'a1'], ['ws-2', 'b1']]))
    store.set(activeWorkspaceIdAtom, 'ws-1')

    const { rerender } = render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBe('a1')

    store.set(activeWorkspaceIdAtom, 'ws-2')
    rerender(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBe('b1')
  })

  it('sets appMode and currentConversationIdAtom for chat tabs', () => {
    const store = createStore()
    store.set(tabsAtom, [mk('c1', 'chat', 'ws-1')])
    store.set(workspaceActiveTabIdMapAtom, new Map([['ws-1', 'c1']]))
    store.set(activeWorkspaceIdAtom, 'ws-1')

    render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(appModeAtom)).toBe('chat')
    expect(store.get(currentConversationIdAtom)).toBe('c1')
  })

  it('clears session atoms when active workspace has no tabs', () => {
    const store = createStore()
    store.set(tabsAtom, [])
    store.set(activeWorkspaceIdAtom, 'ws-empty')
    store.set(currentAgentSessionIdAtom, 'stale')
    render(<Provider store={store}><TabSessionSyncer /></Provider>)
    expect(store.get(currentAgentSessionIdAtom)).toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run TabSessionSyncer.test.tsx`
Expected: FAIL — file doesn't exist.

- [ ] **Step 3: Create `TabSessionSyncer.tsx`**

```tsx
/**
 * TabSessionSyncer — effect-only component that keeps the right
 * panel and chat/agent body content pointed at the active workspace's
 * active tab.
 *
 * Background: per-workspace tab memory (this PR) flips activeTabIdAtom
 * automatically on workspace switch. But the body / right-panel are
 * gated on currentAgentSessionIdAtom / currentConversationIdAtom,
 * which are only written by TabBar's click handler. Without this
 * syncer, the body keeps showing the previous workspace's content.
 *
 * The syncer subscribes to activeTabAtom (derived from the per-workspace
 * map) and writes the matching session/conversation atom + appMode
 * whenever the active tab changes.
 *
 * Mounted once in AppShell. Returns null.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { activeTabAtom } from '@/atoms/tab-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'

export function TabSessionSyncer(): null {
  const activeTab = useAtomValue(activeTabAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setCurrentConversationId = useSetAtom(currentConversationIdAtom)
  const setCurrentAgentSessionId = useSetAtom(currentAgentSessionIdAtom)

  React.useEffect(() => {
    if (!activeTab) {
      // Workspace has no active tab — clear the session atoms so the
      // body/right-panel hides (they treat null as empty-state).
      setCurrentAgentSessionId(null)
      setCurrentConversationId(null)
      return
    }
    if (activeTab.type === 'agent') {
      setAppMode('agent')
      setCurrentAgentSessionId(activeTab.sessionId)
    } else if (activeTab.type === 'chat') {
      setAppMode('chat')
      setCurrentConversationId(activeTab.sessionId)
    }
    // Browser tabs don't change appMode or session atoms — they keep
    // the prior mode (matches TabBar.handleActivate at line 86-88).
  }, [activeTab, setAppMode, setCurrentAgentSessionId, setCurrentConversationId])

  return null
}
```

- [ ] **Step 4: Mount the syncer in AppShell**

Edit `ui/src/components/app-shell/AppShell.tsx`:

```tsx
import { TabSessionSyncer } from './TabSessionSyncer'

// Inside the AppShell component's return — sibling of <App />:
<TabSessionSyncer />
```

Pick a location near the top of the tree (above the routed body, below
any required providers). Inspect AppShell's existing structure to pick
the right spot.

- [ ] **Step 5: Run test to verify it passes**

Run: `cd ui && npm test -- --run TabSessionSyncer.test.tsx`
Expected: PASS (3/3)

- [ ] **Step 6: Smoke test**

In the app:
1. Workspace A, open agent session A1. Right panel shows A1's files.
2. ⌘2 → workspace B. Right panel hides (B has no tabs).
3. Open agent session B1 in B. Right panel shows B1's files.
4. ⌘1 → workspace A. Right panel shows A1's files (not B1's).

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/app-shell/TabSessionSyncer.tsx \
  ui/src/components/app-shell/TabSessionSyncer.test.tsx \
  ui/src/components/app-shell/AppShell.tsx
git commit -m "feat(tabs): TabSessionSyncer — right panel follows workspace switch

Effect-only component mounted once in AppShell. Subscribes to
activeTabAtom (which auto-flips on workspace switch via the
per-workspace map) and writes currentAgentSessionIdAtom /
currentConversationIdAtom / appModeAtom to match.

Without this, the right panel + body would keep showing the
previous workspace's session content after a ⌘N workspace switch,
because those atoms were only written by TabBar's onClick handler.
TabBar's onClick logic is retained — TabSessionSyncer is a defensive
duplicate that handles the non-click path."
```

---

## Task 5: Orphan cleanup on workspace deletion

**Files:**
- Modify: `ui/src/atoms/workspace.ts` (the deleteWorkspaceAtom write
  handler, or wherever workspace deletion lives)

When a workspace is deleted, its tabs become orphans in the global
pool (their `workspaceId` no longer matches any workspace). Add a
cleanup pass that drops them and clears the workspace's entry in
`workspaceActiveTabIdMapAtom`.

- [ ] **Step 1: Locate the workspace-deletion atom**

Run: `grep -n "deleteWorkspace\|removeWorkspace" ui/src/atoms/workspace.ts | head`

- [ ] **Step 2: Write the failing test**

Add to an existing workspace-atom test file (or create
`ui/src/atoms/workspace-tab-cleanup.test.ts`):

```ts
import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { tabsAtom, workspaceActiveTabIdMapAtom, type TabItem } from './tab-atoms'
import { workspacesAtom, type WorkspaceInfo } from './workspace'

function ws(id: string): WorkspaceInfo {
  return {
    id, name: id, icon: 'Folder', path: `/${id}`,
    attachedDirs: [], sortOrder: 0, createdAt: '', updatedAt: '',
  }
}
function tab(id: string, workspaceId: string): TabItem {
  return { id, type: 'agent', sessionId: id, title: id, workspaceId }
}

describe('tab cleanup on workspace deletion', () => {
  it('drops tabs whose workspaceId no longer exists', () => {
    const store = createStore()
    store.set(tabsAtom, [tab('a1', 'ws-1'), tab('b1', 'ws-2')])
    store.set(workspaceActiveTabIdMapAtom, new Map([
      ['ws-1', 'a1'], ['ws-2', 'b1'],
    ]))
    store.set(workspacesAtom, [ws('ws-1'), ws('ws-2')])
    // Simulate deletion: workspacesAtom updated to remove ws-2.
    store.set(workspacesAtom, [ws('ws-1')])
    // Cleanup hook should have fired:
    expect(store.get(tabsAtom).map((t) => t.id)).toEqual(['a1'])
    expect(store.get(workspaceActiveTabIdMapAtom).has('ws-2')).toBe(false)
  })
})
```

- [ ] **Step 3: Implement the cleanup**

Cleanest implementation: an effect-only `WorkspaceTabCleaner`
component mounted in AppShell (same pattern as TabSessionSyncer). It
watches `workspacesAtom` and on every change recomputes the orphan
set.

Alternative: hook into the existing `deleteWorkspaceAtom` write
handler if one exists. Pick whichever has the lower blast radius
based on how the codebase currently structures workspace mutations.

Pattern:

```tsx
// ui/src/components/app-shell/WorkspaceTabCleaner.tsx
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { workspacesAtom } from '@/atoms/workspace'
import { tabsAtom, workspaceActiveTabIdMapAtom } from '@/atoms/tab-atoms'

export function WorkspaceTabCleaner(): null {
  const workspaces = useAtomValue(workspacesAtom)
  const setTabs = useSetAtom(tabsAtom)
  const setActiveMap = useSetAtom(workspaceActiveTabIdMapAtom)

  React.useEffect(() => {
    const live = new Set(workspaces.map((w) => w.id))
    setTabs((prev) => prev.filter((t) => live.has(t.workspaceId)))
    setActiveMap((prev) => {
      let mutated = false
      const next = new Map(prev)
      for (const k of next.keys()) {
        if (!live.has(k)) { next.delete(k); mutated = true }
      }
      return mutated ? next : prev
    })
  }, [workspaces, setTabs, setActiveMap])

  return null
}
```

Mount in AppShell alongside TabSessionSyncer.

- [ ] **Step 4: Run tests**

Run: `cd ui && npm test -- --run`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/app-shell/WorkspaceTabCleaner.tsx \
  ui/src/components/app-shell/AppShell.tsx \
  ui/src/atoms/workspace-tab-cleanup.test.ts  # if created
git commit -m "feat(tabs): orphan cleanup on workspace deletion

Mount a WorkspaceTabCleaner effect-only component in AppShell that
watches workspacesAtom. On every change, drop tabs whose workspaceId
is no longer present and clear stale entries in
workspaceActiveTabIdMapAtom.

Without this, deleting a workspace would leak orphan tabs into the
flat tabsAtom pool — they'd never be visible (the visibleTabsAtom
filter hides them) but would keep accumulating across the session,
and could resurrect if the user later created a workspace with a
colliding id."
```

---

## Task 6: Final integration tests + polish

**Files:**
- Modify: `ui/src/atoms/tab-atoms.test.ts` (add orphan + cleanup cases)
- Modify: `ui/src/components/tabs/TabBar.test.tsx` (add cross-workspace
  visibility flip)

- [ ] **Step 1: Cross-workspace tab-flip integration test**

Add a comprehensive test in `TabBar.test.tsx`:

```tsx
it('switching workspace flips both visible tabs AND active tab id', () => {
  const store = createStore()
  store.set(tabsAtom, [mk('a1', 'ws-1'), mk('a2', 'ws-1'), mk('b1', 'ws-2')])
  store.set(workspaceActiveTabIdMapAtom, new Map([
    ['ws-1', 'a2'], ['ws-2', 'b1'],
  ]))
  store.set(workspacesAtom, [
    { id: 'ws-1', name: 'A', icon: 'Folder', path: '/a', attachedDirs: [], sortOrder: 0, createdAt: '', updatedAt: '' },
    { id: 'ws-2', name: 'B', icon: 'Folder', path: '/b', attachedDirs: [], sortOrder: 1, createdAt: '', updatedAt: '' },
  ])
  store.set(activeWorkspaceIdAtom, 'ws-1')

  const { rerender } = render(<Provider store={store}><TabBar /></Provider>)
  // a2 is highlighted as active (per the ws-1 slot)
  expect(screen.getByText('a2').closest('button')).toHaveAttribute('data-active', 'true')

  store.set(activeWorkspaceIdAtom, 'ws-2')
  rerender(<Provider store={store}><TabBar /></Provider>)
  // Only b1 visible, and it's the active one
  expect(screen.queryByText('a1')).not.toBeInTheDocument()
  expect(screen.queryByText('a2')).not.toBeInTheDocument()
  expect(screen.getByText('b1').closest('button')).toHaveAttribute('data-active', 'true')
})
```

(If `data-active` isn't currently used, check what attribute /class
TabBarItem uses to mark active state and assert on that.)

- [ ] **Step 2: Run full UI test suite**

Run: `cd ui && npm test -- --run`
Expected: All pass.

- [ ] **Step 3: Run TS check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -5`
Expected: clean.

- [ ] **Step 4: Manual smoke checklist**

In `cargo tauri dev`, verify:
- [ ] Workspace A: open 2 tabs (chat + agent). Both visible.
- [ ] ⌘2 → Workspace B: tab list empty.
- [ ] Open 1 tab in B. Only B's tab visible.
- [ ] ⌘1 → A: A's 2 tabs back, the previously-active one is active.
- [ ] Right panel: shows A's session content after ⌘1.
- [ ] Drag-reorder a tab in A. ⌘2 to B and back to A: A's order
      persists; B's single tab unchanged.
- [ ] Delete workspace B (via WorkspaceHeader 🗑). B's tabs gone
      from the pool (verify via React DevTools jotai inspector).
- [ ] Restart app. (Tabs reset to empty — that's expected; no
      persistence.)

- [ ] **Step 5: Commit**

```bash
git add ui/src/atoms/tab-atoms.test.ts ui/src/components/tabs/TabBar.test.tsx
git commit -m "test(tabs): cross-workspace flip + orphan cleanup integration

Adds:
- TabBar test that asserts switching activeWorkspaceIdAtom flips
  both visible tab list AND active-tab indicator simultaneously.
- tab-atoms test that asserts orphan cleanup on workspacesAtom
  shrinkage drops tabs + clears workspaceActiveTabIdMapAtom slots.

These guard against the most likely regression: someone adding a
new derived atom or component that subscribes to tabsAtom directly
instead of visibleTabsAtom."
```

---

## After all tasks complete

Use **superpowers:finishing-a-development-branch** to merge.
Expected PR shape:

```
## Summary
Phase 5 of the workspace remediation series. Makes tabs per-workspace
and rewires the right-side panel to follow workspace switches.

## Commits (bisectable)
1. feat(atoms): tag TabItem with workspaceId + per-workspace active-tab map
2. refactor(tabs): TabBar + MainArea + switchers read visibleTabsAtom
3. refactor(tabs): openTab callers pass active workspace id
4. feat(tabs): TabSessionSyncer — right panel follows workspace switch
5. feat(tabs): orphan cleanup on workspace deletion
6. test(tabs): cross-workspace flip + orphan cleanup integration

## Test plan
[Manual smoke checklist from Task 6 Step 4]
```

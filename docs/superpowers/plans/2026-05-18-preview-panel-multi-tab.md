# Preview panel multi-tab — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single-file preview slot with a tab pool where agent-opened files cluster left with a ✨ marker, manual-opened cluster right, duplicates focus existing tabs, and tab close behaves intuitively.

**Architecture:** Mirror Agent View's `tabsAtom` + `activeTabIdAtom` pattern (in-memory pool + active pointer) but bespoke for preview files. The existing `selectedPreviewFileAtom` becomes a *derived* atom reading from the active tab key, so every reader (PreviewPanel, header, hooks) stays unchanged. Three callers that currently invoke `openPreviewAction(target)` migrate to `openPreviewTabAction({ target, source })`; the legacy `openPreviewAction` becomes a manual-source compatibility wrapper so unmigrated code still works.

**Tech Stack:** React 18 + TypeScript, Jotai atoms, Vitest + React Testing Library, Tailwind (theme tokens only), lucide-react icons.

Spec: [docs/superpowers/specs/2026-05-18-preview-panel-multi-tab-design.md](../specs/2026-05-18-preview-panel-multi-tab-design.md).

---

## File map (locked in before coding)

**Modified:**
- `ui/src/atoms/preview-panel-atoms.ts` — add `previewTabsAtom`, `activePreviewTabKeyAtom`, `openPreviewTabAction`, `closePreviewTabAction`, `clearAllPreviewTabsAction`, `previewTabKey()`, `sortPreviewTabs()`; convert `selectedPreviewFileAtom` to derived; rewire `openPreviewAction` as compat wrapper
- `ui/src/atoms/preview-panel-atoms.test.ts` — NEW file with action-level tests
- `ui/src/components/preview/PreviewTabBar.tsx` — NEW
- `ui/src/components/preview/PreviewTabBar.test.tsx` — NEW
- `ui/src/components/preview/PreviewTabItem.tsx` — NEW
- `ui/src/components/preview/PreviewPanel.tsx` — mount `<PreviewTabBar>` above existing content
- `ui/src/hooks/useGlobalAgentListeners.ts` — agent-source migration at L485
- `ui/src/components/preview/chips/FilePathChip.tsx` — manual-source migration at L45
- `ui/src/components/agent/SidePanel.tsx` — manual-source migration at L82
- `ui/src/views/Workspace/WorkspaceShell.tsx` — workspace-reset migration at L39+

**Unchanged but worth verifying compatibility:**
- `ui/src/components/preview/PreviewHeader.tsx` — reads `PreviewFileTarget` type only
- `ui/src/components/preview/hooks/usePreviewState.ts` — reads `selectedPreviewFileAtom` (now derived; same shape)
- `ui/src/atoms/preview-editor-atoms.ts` — dirty-buffer map; semantics shift (dirty prompt fires on tab close, not on switch)

---

## Task 1 — Multi-tab atoms + actions

**Files:**
- Modify: `ui/src/atoms/preview-panel-atoms.ts`
- Create: `ui/src/atoms/preview-panel-atoms.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `ui/src/atoms/preview-panel-atoms.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  type PreviewFileTarget,
  type PreviewTabItem,
  previewTabsAtom,
  activePreviewTabKeyAtom,
  previewPanelOpenAtom,
  selectedPreviewFileAtom,
  openPreviewTabAction,
  closePreviewTabAction,
  clearAllPreviewTabsAction,
  previewTabKey,
  openPreviewAction,
} from './preview-panel-atoms'

const FOO: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'foo.md',
  name: 'foo.md',
  absolutePath: '/abs/foo.md',
  sessionId: 's1',
}
const BAR: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'bar.md',
  name: 'bar.md',
  absolutePath: '/abs/bar.md',
  sessionId: 's1',
}
const BAZ: PreviewFileTarget = {
  mountId: 'workspace:default',
  relPath: 'baz.md',
  name: 'baz.md',
  absolutePath: '/abs/baz.md',
  sessionId: 's1',
}

describe('previewTabKey', () => {
  it('composes mountId and relPath', () => {
    expect(previewTabKey({ mountId: 'm', relPath: 'a/b.md' })).toBe('m:a/b.md')
  })
})

describe('openPreviewTabAction', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-05-18T00:00:00Z'))
  })

  it('inserts a new tab, activates it, opens the panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    expect(store.get(previewTabsAtom)).toHaveLength(1)
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
    expect(store.get(previewPanelOpenAtom)).toBe(true)
  })

  it('focuses existing tab when same key is opened again (no duplicate)', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    // Re-open foo
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    expect(store.get(previewTabsAtom)).toHaveLength(2)
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('agent-source tabs cluster left of manual tabs', () => {
    const store = createStore()
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAZ, source: 'manual' })
    const order = store.get(previewTabsAtom).map((t) => t.name)
    expect(order).toEqual(['bar.md', 'foo.md', 'baz.md'])
  })

  it('promotes a manual tab to agent on agent re-open and re-sorts', () => {
    const store = createStore()
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    vi.advanceTimersByTime(1)
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    // BAR re-opened by agent → promote BAR, it moves left of FOO (manual)
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    const order = store.get(previewTabsAtom).map((t) => t.name)
    expect(order).toEqual(['bar.md', 'foo.md'])
    const barTab = store.get(previewTabsAtom).find((t) => t.name === 'bar.md')
    expect(barTab?.source).toBe('agent')
  })
})

describe('closePreviewTabAction', () => {
  it('removes the tab and activates the neighbor on the right', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAZ, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md', 'baz.md'])
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:baz.md')
  })

  it('falls back to left neighbor when right neighbor missing', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:bar.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('closing the last tab nulls active and closes the panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(closePreviewTabAction, 'workspace:default:foo.md')
    expect(store.get(previewTabsAtom)).toHaveLength(0)
    expect(store.get(activePreviewTabKeyAtom)).toBeNull()
    expect(store.get(previewPanelOpenAtom)).toBe(false)
  })

  it('closing an inactive tab leaves the active tab unchanged', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    store.set(activePreviewTabKeyAtom, 'workspace:default:foo.md')
    store.set(closePreviewTabAction, 'workspace:default:bar.md')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })

  it('no-op when key not found', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(closePreviewTabAction, 'nonexistent:key')
    expect(store.get(previewTabsAtom)).toHaveLength(1)
  })
})

describe('selectedPreviewFileAtom (derived)', () => {
  it('returns null when no tab is active', () => {
    const store = createStore()
    expect(store.get(selectedPreviewFileAtom)).toBeNull()
  })

  it('returns the active tab projected as PreviewFileTarget', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    const sel = store.get(selectedPreviewFileAtom)
    expect(sel).toEqual({
      mountId: FOO.mountId,
      relPath: FOO.relPath,
      name: FOO.name,
      absolutePath: FOO.absolutePath,
      sessionId: FOO.sessionId,
    })
  })

  it('updates when the active tab changes', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'manual' })
    // BAR is now active (just opened)
    expect(store.get(selectedPreviewFileAtom)?.name).toBe('bar.md')
    store.set(activePreviewTabKeyAtom, 'workspace:default:foo.md')
    expect(store.get(selectedPreviewFileAtom)?.name).toBe('foo.md')
  })
})

describe('openPreviewAction (legacy compat wrapper)', () => {
  it('delegates to openPreviewTabAction with source manual', () => {
    const store = createStore()
    store.set(openPreviewAction, FOO)
    expect(store.get(previewTabsAtom)).toHaveLength(1)
    const tab = store.get(previewTabsAtom)[0] as PreviewTabItem
    expect(tab.source).toBe('manual')
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })
})

describe('clearAllPreviewTabsAction', () => {
  it('removes all tabs, nulls active, closes panel', () => {
    const store = createStore()
    store.set(openPreviewTabAction, { target: FOO, source: 'manual' })
    store.set(openPreviewTabAction, { target: BAR, source: 'agent' })
    store.set(clearAllPreviewTabsAction)
    expect(store.get(previewTabsAtom)).toHaveLength(0)
    expect(store.get(activePreviewTabKeyAtom)).toBeNull()
    expect(store.get(previewPanelOpenAtom)).toBe(false)
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL (atoms/actions don't exist yet)**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab/ui && npm test -- --run preview-panel-atoms 2>&1 | tail -20
```

Expected: compile error / 14 tests fail with "not exported from preview-panel-atoms".

- [ ] **Step 3: Replace the relevant portion of `preview-panel-atoms.ts`**

Open `ui/src/atoms/preview-panel-atoms.ts`. Find the `export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(null)` line (around L29) and the `export const openPreviewAction = ...` block (around L58-81). Replace this whole region with the new multi-tab implementation.

Before edit — current shape:

```ts
export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(null)
export const previewPanelOpenAtom = atom<boolean>(false)
// ... (other atoms stay)
export const openPreviewAction = atom(null, (get, set, payload: PreviewFileTarget) => {
  // dirty-buffer prompt + set + open
})
```

After edit — replace with:

```ts
// ── Multi-tab pool ─────────────────────────────────────────────────────
//
// Multi-file preview: the panel keeps a tab pool keyed by mountId:relPath.
// Agent-source tabs cluster left with a ✨ marker (the agent's outputs are
// what the user wants to inspect first); manual-source tabs cluster right.
// Re-opening the same file focuses the existing tab — never duplicates.
// In-memory only — matches the chat-session tabsAtom convention.

export type PreviewTabSource = 'agent' | 'manual'

export interface PreviewTabItem {
  /** Composite identity: two tabs with same (mountId, relPath) are merged. */
  mountId: string
  relPath: string
  /** Display name (last path segment, mirror of PreviewFileTarget.name). */
  name: string
  /** Absolute path; '' if unresolved (renderer handles that case). */
  absolutePath?: string
  /** Session that owns the mount, when relevant. */
  sessionId?: string | null
  /** Determines sort cluster + visual marker. */
  source: PreviewTabSource
  /** Insertion epoch — tiebreaker within source group. */
  addedAt: number
}

export const previewTabsAtom = atom<PreviewTabItem[]>([])

/** `${mountId}:${relPath}` for the currently active tab, or null. */
export const activePreviewTabKeyAtom = atom<string | null>(null)

/** Stable composite key used everywhere the tab list is indexed. */
export function previewTabKey(
  t: Pick<PreviewTabItem, 'mountId' | 'relPath'>,
): string {
  return `${t.mountId}:${t.relPath}`
}

/** Agent tabs first (by addedAt asc), then manual (by addedAt asc). */
function sortPreviewTabs(tabs: PreviewTabItem[]): PreviewTabItem[] {
  return [...tabs].sort((a, b) => {
    if (a.source !== b.source) return a.source === 'agent' ? -1 : 1
    return a.addedAt - b.addedAt
  })
}

// ── selectedPreviewFileAtom — derived from active tab ──────────────────
//
// All existing readers of this atom (PreviewPanel, PreviewHeader,
// usePreviewState) continue to work; they see PreviewFileTarget | null
// just like before, but the source of truth is now the tab pool.

export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>((get) => {
  const key = get(activePreviewTabKeyAtom)
  if (!key) return null
  const tab = get(previewTabsAtom).find((t) => previewTabKey(t) === key)
  if (!tab) return null
  return {
    mountId: tab.mountId,
    relPath: tab.relPath,
    name: tab.name,
    absolutePath: tab.absolutePath,
    sessionId: tab.sessionId,
  }
})

export const previewPanelOpenAtom = atom<boolean>(false)

// ── Actions ────────────────────────────────────────────────────────────

/**
 * Open a file in a new tab, or focus the existing tab if already open.
 *
 * Source semantics:
 *  - 'agent'  → tab clusters left, carries ✨ marker. If a manual tab for
 *               the same file already exists, it gets promoted to 'agent'
 *               and migrates left.
 *  - 'manual' → tab clusters right. Re-opening an existing agent tab does
 *               NOT demote it (agent priority is sticky).
 *
 * Always sets the tab active and opens the panel.
 *
 * Note: the dirty-buffer confirm prompt that the legacy single-file
 * openPreviewAction used now lives on closePreviewTabAction (where it
 * belongs — switching tabs in a multi-tab pane should NOT discard buffers).
 */
export const openPreviewTabAction = atom(
  null,
  (
    get,
    set,
    payload: { target: PreviewFileTarget; source: PreviewTabSource },
  ) => {
    const tabs = get(previewTabsAtom)
    const key = previewTabKey(payload.target)
    const existing = tabs.find((t) => previewTabKey(t) === key)
    if (existing) {
      set(activePreviewTabKeyAtom, key)
      if (payload.source === 'agent' && existing.source === 'manual') {
        set(
          previewTabsAtom,
          sortPreviewTabs(
            tabs.map((t) =>
              previewTabKey(t) === key ? { ...t, source: 'agent' as const } : t,
            ),
          ),
        )
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
  },
)

/**
 * Close a tab by its composite key.
 *  - If closing the active tab, activates the right neighbor (or left, or null)
 *  - If closing the last tab, also closes the panel
 *  - If the closing tab has a dirty editor buffer, prompts the user;
 *    on cancel, leaves the tab open
 *
 * No-op if the key isn't found.
 */
export const closePreviewTabAction = atom(
  null,
  (get, set, key: string) => {
    const tabs = get(previewTabsAtom)
    const idx = tabs.findIndex((t) => previewTabKey(t) === key)
    if (idx === -1) return
    const closingTab = tabs[idx]

    // Dirty-buffer confirmation (only when CLOSING this tab — switching
    // active tab between still-open tabs leaves buffers untouched).
    const buffers = get(dirtyBuffersAtom)
    const path = closingTab.absolutePath ?? null
    if (path && buffers.has(path)) {
      const proceed = window.confirm(
        '该文件有未保存的修改 — 关闭这个标签将丢弃这些修改。是否继续？',
      )
      if (!proceed) return
      const nextBuffers = new Map(buffers)
      nextBuffers.delete(path)
      set(dirtyBuffersAtom, nextBuffers)
    }

    const next = tabs.filter((t) => previewTabKey(t) !== key)
    set(previewTabsAtom, next)
    if (get(activePreviewTabKeyAtom) === key) {
      const neighbor = next[idx] ?? next[idx - 1] ?? null
      set(activePreviewTabKeyAtom, neighbor ? previewTabKey(neighbor) : null)
      if (next.length === 0) {
        set(previewPanelOpenAtom, false)
      }
    }
  },
)

/** Close every tab + collapse the panel. Used on workspace switch. */
export const clearAllPreviewTabsAction = atom(null, (_get, set) => {
  set(previewTabsAtom, [])
  set(activePreviewTabKeyAtom, null)
  set(previewPanelOpenAtom, false)
})

// ── Compatibility wrapper ──────────────────────────────────────────────
//
// Legacy callers do `set(openPreviewAction, target)`. They still work —
// new tabs default to source: 'manual'. New callers should use
// openPreviewTabAction explicitly with the right source.
export const openPreviewAction = atom(
  null,
  (_get, set, target: PreviewFileTarget) => {
    set(openPreviewTabAction, { target, source: 'manual' })
  },
)
```

The `closePreviewAction` block (further down in the file, around L124-147) stays as-is — it's the WHOLE-PANEL close (close button + Esc), not the per-tab close. It needs to clear all tabs when collapsing the panel. Adjust the end of that block:

After `set(previewPanelOpenAtom, false)`, add a line so the tab pool clears alongside:

```ts
  set(previewPanelOpenAtom, false)
  set(previewTabsAtom, [])              // ← NEW
  set(activePreviewTabKeyAtom, null)    // ← NEW
})
```

- [ ] **Step 4: Re-run tests — expect GREEN**

```bash
cd ui && npm test -- --run preview-panel-atoms 2>&1 | tail -10
```

Expected: `Tests  14 passed (14)`.

- [ ] **Step 5: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -20
```

Expected: clean. If any error about `dirtyBuffersAtom` not in scope, ensure the existing `import { dirtyBuffersAtom } from './preview-editor-atoms'` at the top of `preview-panel-atoms.ts` is still present.

- [ ] **Step 6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/atoms/preview-panel-atoms.ts \
  ui/src/atoms/preview-panel-atoms.test.ts

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): multi-tab atoms + actions

New atoms:
  - previewTabsAtom (PreviewTabItem[], in-memory)
  - activePreviewTabKeyAtom (composite key or null)

New actions:
  - openPreviewTabAction({target, source}) — open or focus, with
    agent-source promotion + sort
  - closePreviewTabAction(key) — neighbor-activate, panel-collapse on
    empty, dirty-buffer confirm
  - clearAllPreviewTabsAction — workspace-switch reset

selectedPreviewFileAtom converted to derived (reads active tab) —
every existing reader (PreviewPanel, PreviewHeader, usePreviewState)
keeps working.

openPreviewAction kept as manual-source compatibility wrapper so
existing callers (FilePathChip, SidePanel) keep working until their
explicit migration in later tasks.

closePreviewAction (whole-panel close) updated to clear the tab
pool alongside the panel-open flag.

14 unit tests cover: key composition, insert-new / focus-existing,
agent-cluster ordering, manual→agent promotion, neighbor-activate
on close, last-tab-closes-panel, derived selectedPreviewFileAtom
reactivity, legacy wrapper delegation, clearAll."
```

---

## Task 2 — `PreviewTabBar` + `PreviewTabItem` components

**Files:**
- Create: `ui/src/components/preview/PreviewTabItem.tsx`
- Create: `ui/src/components/preview/PreviewTabBar.tsx`
- Create: `ui/src/components/preview/PreviewTabBar.test.tsx`

- [ ] **Step 1: Find the existing file-type icon helper**

```bash
grep -rn "FileTypeIcon\|getFileTypeIcon\|fileTypeIcon" ui/src/components --include="*.tsx" --include="*.ts" | head -5
```

You should find `ui/src/components/file-browser/FileTypeIcon.tsx`. Inspect what it exports (likely `FileTypeIcon` component taking a name/path prop). Use that in the tab item.

- [ ] **Step 2: Create `PreviewTabItem.tsx`**

```tsx
import * as React from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import type { PreviewTabItem as PreviewTabItemModel } from '@/atoms/preview-panel-atoms'

interface Props {
  tab: PreviewTabItemModel
  isActive: boolean
  onActivate: () => void
  onClose: () => void
}

export function PreviewTabItem({
  tab,
  isActive,
  onActivate,
  onClose,
}: Props): React.ReactElement {
  return (
    <div
      role="tab"
      aria-selected={isActive}
      aria-label={tab.name}
      tabIndex={isActive ? 0 : -1}
      onClick={onActivate}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onActivate()
        }
      }}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault()
          onClose()
        }
      }}
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 text-xs cursor-pointer select-none',
        'border-r border-border/40 min-w-[80px] max-w-[200px] shrink-0',
        isActive
          ? 'bg-background text-foreground border-b-2 border-b-primary'
          : 'bg-card text-muted-foreground hover:bg-muted/40',
      )}
    >
      {tab.source === 'agent' && (
        <span
          aria-label="opened by agent"
          title="opened by agent"
          className="text-[10px] leading-none"
        >
          ✨
        </span>
      )}
      <FileTypeIcon name={tab.name} className="size-3.5 shrink-0" />
      <span className="truncate flex-1" title={tab.relPath}>
        {tab.name}
      </span>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          onClose()
        }}
        className={cn(
          'size-4 flex items-center justify-center rounded shrink-0',
          'opacity-0 group-hover:opacity-100',
          isActive && 'opacity-100',
          'hover:bg-muted/60 transition-opacity',
        )}
        aria-label={`close ${tab.name}`}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}
```

If `FileTypeIcon` doesn't take a `name` prop or a `className`, adapt to its actual API (read the file before importing). If `FileTypeIcon` is unavailable, fall back to a plain `<FileText className="size-3.5" />` from lucide-react.

- [ ] **Step 3: Create `PreviewTabBar.tsx`**

```tsx
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  previewTabsAtom,
  activePreviewTabKeyAtom,
  closePreviewTabAction,
  previewTabKey,
} from '@/atoms/preview-panel-atoms'
import { PreviewTabItem } from './PreviewTabItem'

export function PreviewTabBar(): React.ReactElement | null {
  const tabs = useAtomValue(previewTabsAtom)
  const activeKey = useAtomValue(activePreviewTabKeyAtom)
  const setActive = useSetAtom(activePreviewTabKeyAtom)
  const closeTab = useSetAtom(closePreviewTabAction)

  if (tabs.length === 0) return null

  return (
    <div
      role="tablist"
      aria-label="预览文件标签页"
      className="flex items-stretch border-b border-border bg-card overflow-x-auto"
    >
      {tabs.map((tab) => {
        const key = previewTabKey(tab)
        return (
          <PreviewTabItem
            key={key}
            tab={tab}
            isActive={key === activeKey}
            onActivate={() => setActive(key)}
            onClose={() => closeTab(key)}
          />
        )
      })}
    </div>
  )
}
```

- [ ] **Step 4: Create the failing tests**

`ui/src/components/preview/PreviewTabBar.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { PreviewTabBar } from './PreviewTabBar'
import {
  previewTabsAtom,
  activePreviewTabKeyAtom,
  previewPanelOpenAtom,
  type PreviewTabItem,
} from '@/atoms/preview-panel-atoms'

const AGENT_FOO: PreviewTabItem = {
  mountId: 'workspace:default',
  relPath: 'foo.md',
  name: 'foo.md',
  absolutePath: '/abs/foo.md',
  sessionId: 's1',
  source: 'agent',
  addedAt: 100,
}
const MANUAL_BAR: PreviewTabItem = {
  mountId: 'workspace:default',
  relPath: 'bar.md',
  name: 'bar.md',
  absolutePath: '/abs/bar.md',
  sessionId: 's1',
  source: 'manual',
  addedAt: 200,
}

function renderWith(
  tabs: PreviewTabItem[],
  activeKey: string | null,
): { store: ReturnType<typeof createStore> } {
  const store = createStore()
  store.set(previewTabsAtom, tabs)
  store.set(activePreviewTabKeyAtom, activeKey)
  store.set(previewPanelOpenAtom, true)
  render(
    <Provider store={store}>
      <PreviewTabBar />
    </Provider>,
  )
  return { store }
}

describe('PreviewTabBar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders nothing when there are 0 tabs', () => {
    const { container } = render(
      <Provider store={createStore()}>
        <PreviewTabBar />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders one tab per item with correct active state', () => {
    renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:bar.md')
    const tabs = screen.getAllByRole('tab')
    expect(tabs).toHaveLength(2)
    expect(tabs[0]).toHaveAttribute('aria-selected', 'false') // foo
    expect(tabs[1]).toHaveAttribute('aria-selected', 'true') // bar (active)
  })

  it('shows the agent ✨ marker only on agent-source tabs', () => {
    renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    // ✨ is wrapped in a span with the title 'opened by agent'
    expect(screen.getAllByTitle('opened by agent')).toHaveLength(1)
  })

  it('clicking a tab activates it', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('bar.md'))
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:bar.md')
  })

  it('close X click removes the tab from the pool', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('close bar.md'))
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md'])
  })

  it('middle-click on tab closes it', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.auxClick(screen.getByLabelText('bar.md'), { button: 1 })
    expect(store.get(previewTabsAtom).map((t) => t.name)).toEqual(['foo.md'])
  })

  it('close click does NOT also activate that tab (stopPropagation works)', () => {
    const { store } = renderWith([AGENT_FOO, MANUAL_BAR], 'workspace:default:foo.md')
    fireEvent.click(screen.getByLabelText('close bar.md'))
    // foo stays active, bar gone — not "bar activated then closed"
    expect(store.get(activePreviewTabKeyAtom)).toBe('workspace:default:foo.md')
  })
})
```

- [ ] **Step 5: Run tests**

```bash
cd ui && npm test -- --run PreviewTabBar 2>&1 | tail -15
```

Expected: `Tests  7 passed (7)`. If `FileTypeIcon` import fails in test env, mock it inside the test file:

```ts
vi.mock('@/components/file-browser/FileTypeIcon', () => ({
  FileTypeIcon: ({ className }: { className?: string }) => <span data-testid="file-icon" className={className} />,
}))
```

- [ ] **Step 6: TypeScript check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/components/preview/PreviewTabItem.tsx \
  ui/src/components/preview/PreviewTabBar.tsx \
  ui/src/components/preview/PreviewTabBar.test.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): PreviewTabBar + PreviewTabItem components

Lightweight tab strip rendered above the preview content. NOT a
reuse of the workspace-scoped TabBar (too coupled to session
lifecycle for a preview-only context).

Per-tab item shows: file icon, name (truncated), agent ✨ marker
on agent-source tabs, close X (visible on hover or when active).
Active tab gets bg-background + bottom border accent; inactive
tabs sit at bg-card + muted-foreground.

Interactions:
  - Click tab → activate
  - Enter/Space on focused tab → activate (keyboard a11y)
  - Click close X → close (stopPropagation prevents activating)
  - Middle-click on tab → close (standard tab UX)

role='tablist' / role='tab' / aria-selected wired correctly.

7 RTL tests cover: empty-state, count + active rendering, agent
marker, click-to-activate, close-X removal, middle-click close,
close-stopPropagation."
```

---

## Task 3 — Mount `<PreviewTabBar>` in `PreviewPanel`

**Files:**
- Modify: `ui/src/components/preview/PreviewPanel.tsx`

- [ ] **Step 1: Inspect the current `PreviewPanel.tsx`**

```bash
sed -n '1,80p' ui/src/components/preview/PreviewPanel.tsx
```

Note the JSX root and how the existing content (PreviewHeader + the renderer chain) is composed. The tab bar should sit BETWEEN the existing header (which shows the active file's name + path) and the renderer body.

Actually re-evaluate: with multi-tab, the header's "current file name" is redundant with the active tab. Two options:
- (a) Tab bar replaces the header's title row, but keep the toolbar (refresh, open-in-editor, etc.)
- (b) Tab bar above the header — accept the redundancy for now (simpler change)

Go with **(b)** to keep this task surgical. A future PR can collapse the redundancy.

- [ ] **Step 2: Add the import + mount**

In `ui/src/components/preview/PreviewPanel.tsx`, at the top of the imports add:

```tsx
import { PreviewTabBar } from './PreviewTabBar'
```

Locate the outermost returned `<div>` (likely something like `<div className="flex flex-col h-full">`). The existing children probably include `<PreviewHeader />` and a renderer-content area. Insert `<PreviewTabBar />` as the FIRST child of the flex-col container, before `<PreviewHeader />`:

```tsx
return (
  <div className="flex flex-col h-full">
    <PreviewTabBar />          {/* NEW */}
    <PreviewHeader ... />
    {/* ... existing content body ... */}
  </div>
)
```

If the existing root isn't a flex-col, wrap with one — but be cautious about height layout breakage. The conservative move: if the root is `<div className="...">`, just prepend `<PreviewTabBar />` and accept whatever the parent layout does. The tab bar is `shrink-0`-style (no `flex-1`), so it should size to content naturally.

- [ ] **Step 3: Visual check via tsc + tests**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run PreviewPanel 2>&1 | tail -10
```

Expected: clean tsc; if there's a `PreviewPanel.test.tsx`, all existing tests still pass (the tab bar is a no-op when 0 tabs exist, so the renderer chain is unaffected).

- [ ] **Step 4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/components/preview/PreviewPanel.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): mount PreviewTabBar above PreviewPanel content

The tab bar renders nothing (returns null) when 0 tabs are open, so
this is effectively a no-op until Task 4-6 wire openPreviewTabAction
into the callers. Existing PreviewHeader stays — for now the active
tab's filename appears both in the tab and the header. A future PR
can collapse that redundancy."
```

---

## Task 4 — Migrate `useGlobalAgentListeners.ts` to agent-source

**Files:**
- Modify: `ui/src/hooks/useGlobalAgentListeners.ts` (around L485)

- [ ] **Step 1: Inspect the existing call**

```bash
sed -n '475,495p' ui/src/hooks/useGlobalAgentListeners.ts
```

You should find a line like `store.set(openPreviewAction, resolved)` where `resolved: PreviewFileTarget`. This is the agent's auto-preview-open trigger.

- [ ] **Step 2: Replace with explicit agent-source call**

Change the import (top of the file) to also include `openPreviewTabAction`:

```ts
import {
  openPreviewAction,       // keep import — may be used elsewhere in the file
  openPreviewTabAction,    // NEW
  type PreviewFileTarget,
  // ... other existing imports
} from '@/atoms/preview-panel-atoms'
```

At the auto-preview call site (~L485):

```ts
// BEFORE:
store.set(openPreviewAction, resolved)

// AFTER:
store.set(openPreviewTabAction, { target: resolved, source: 'agent' })
```

If `openPreviewAction` was the ONLY use of that import in the file, drop it from the import list to keep things tidy.

- [ ] **Step 3: TypeScript + smoke test**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run useGlobalAgentListeners 2>&1 | tail -10
```

Expected: tsc clean; existing tests pass (auto-preview behavior preserved, just now it inserts into the tab pool with source='agent').

- [ ] **Step 4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/hooks/useGlobalAgentListeners.ts

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): agent auto-preview opens as agent-source tab

When the agent writes/edits a file and the dispatcher's
chat:stream-tool-activity payload resolves a previewTarget,
useGlobalAgentListeners now calls openPreviewTabAction with
source='agent' instead of the legacy openPreviewAction. The new
tab inserts to the LEFT of any existing manual-source tabs and
carries a ✨ marker so the user can spot agent outputs at a glance."
```

---

## Task 5 — Migrate `FilePathChip.tsx` + `SidePanel.tsx` to manual-source

**Files:**
- Modify: `ui/src/components/preview/chips/FilePathChip.tsx` (around L45)
- Modify: `ui/src/components/agent/SidePanel.tsx` (around L29, L82)

These two are both manual-source callers; one commit suffices.

- [ ] **Step 1: FilePathChip migration**

Open `ui/src/components/preview/chips/FilePathChip.tsx`. Find the click handler that calls `openPreview(...)` (around L45 declaration + the actual `openPreview({...})` call further down).

In the imports, swap:

```ts
// BEFORE:
import { openPreviewAction } from '@/atoms/preview-panel-atoms'
// AFTER:
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'
```

In the component body, swap the `useSetAtom` target:

```ts
// BEFORE:
const openPreview = useSetAtom(openPreviewAction)
// AFTER:
const openPreview = useSetAtom(openPreviewTabAction)
```

At each call site of `openPreview(...)`, change from passing the target directly to wrapping in the new payload shape:

```ts
// BEFORE:
openPreview({
  mountId, relPath, name, sessionId, absolutePath,
})
// AFTER:
openPreview({
  target: { mountId, relPath, name, sessionId, absolutePath },
  source: 'manual',
})
```

Some FilePathChip click handlers call `openPreview` with an object literal that already matches `PreviewFileTarget` — wrap that literal in `{ target: …, source: 'manual' }`.

- [ ] **Step 2: SidePanel migration**

Open `ui/src/components/agent/SidePanel.tsx`. Same pattern:

```ts
// L12: change import
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'

// L29: change useSetAtom target
const openPreview = useSetAtom(openPreviewTabAction)

// L82-88: wrap the call
openPreview({
  target: {
    mountId: mount.id,
    relPath: node.relPath,
    name: node.name,
    sessionId,
    absolutePath: `${mount.path}/${node.relPath}`,
  },
  source: 'manual',
})
```

- [ ] **Step 3: tsc + tests**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run "FilePathChip|SidePanel" 2>&1 | tail -15
```

Expected: clean tsc; any existing tests for these components pass (manual-source produces the same UX as before — the wrapped action ends up at the same atoms).

- [ ] **Step 4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/components/preview/chips/FilePathChip.tsx \
  ui/src/components/agent/SidePanel.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): FilePathChip + SidePanel file-rail open as manual-source tab

Explicit source: 'manual' makes the intent clear at the call site
and ensures these clicks cluster RIGHT of any agent-source tabs.

The legacy openPreviewAction wrapper would have produced the same
behavior implicitly (it defaults to manual) — going explicit lets
us delete the wrapper in a future cleanup PR without scanning callers."
```

---

## Task 6 — Migrate `WorkspaceShell.tsx` workspace-switch reset

**Files:**
- Modify: `ui/src/views/Workspace/WorkspaceShell.tsx` (around L39 + the workspace-switch effect)

- [ ] **Step 1: Inspect the existing reset effect**

```bash
sed -n '35,75p' ui/src/views/Workspace/WorkspaceShell.tsx
```

You'll see an effect that on workspace change does something like `setSelectedPreviewFile(null)` + `setPreviewOpen(false)`. With multi-tab, leaving stale tabs from the previous workspace in the pool would surface unresolvable files. We need to clear the whole pool.

- [ ] **Step 2: Replace the imports + setter**

Top of file:

```ts
// BEFORE:
import {
  selectedPreviewFileAtom,
  previewPanelOpenAtom,
  // ... other imports
} from '@/atoms/preview-panel-atoms'

// AFTER:
import {
  clearAllPreviewTabsAction,
  previewPanelOpenAtom,
  previewPanelSplitRatioAtom,
  // ... other existing imports — KEEP whatever's there besides selectedPreviewFileAtom
} from '@/atoms/preview-panel-atoms'
```

If selectedPreviewFileAtom is used elsewhere in this file (not just for clearing on switch), keep the import — only drop it if `setSelectedPreviewFile` was its only use.

In the component body:

```ts
// BEFORE:
const setSelectedPreviewFile = useSetAtom(selectedPreviewFileAtom)
// AFTER:
const clearAllTabs = useSetAtom(clearAllPreviewTabsAction)
```

In the workspace-change effect (the one with `prevWorkspaceRef`):

```ts
// BEFORE:
if (prevWorkspaceRef.current !== currentWorkspaceId) {
  setSelectedPreviewFile(null)
  setPreviewOpen(false)
  prevWorkspaceRef.current = currentWorkspaceId
}

// AFTER:
if (prevWorkspaceRef.current !== currentWorkspaceId) {
  clearAllTabs()
  prevWorkspaceRef.current = currentWorkspaceId
}
```

`clearAllPreviewTabsAction` already resets `previewPanelOpenAtom` to false, so the explicit `setPreviewOpen(false)` is redundant. Drop the `setPreviewOpen` line ONLY if it's no longer used elsewhere in the file; if it is, leave the line but expect it to be a no-op after clearAllTabs runs.

- [ ] **Step 3: tsc + tests**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run WorkspaceShell 2>&1 | tail -10
```

Expected: clean tsc; existing tests pass (workspace switch still closes the panel, now also nukes the tab pool).

- [ ] **Step 4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab add \
  ui/src/views/Workspace/WorkspaceShell.tsx

git -C /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab commit -m "feat(preview): workspace switch clears all preview tabs

Direct setSelectedPreviewFile(null) replaced with the new
clearAllPreviewTabsAction so the whole tab pool is wiped on
workspace change. Tabs from the previous workspace can't resolve
their mounts in the new one and would surface confusing 'file
missing' states otherwise.

setPreviewOpen(false) is removed because clearAllPreviewTabsAction
already collapses the panel; keeping it would be dead code."
```

---

## Task 7 — Final verification + integration

**Files:**
- No code changes; verification only

- [ ] **Step 1: Full test suite — should be 980+ tests with the new ones added**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab/ui && npm test -- --run 2>&1 | tail -10
```

Expected: same passing count as main + ~21 new tests (14 atoms + 7 banner). Some pre-existing flaky tests (SearchPalette / GeneralTab / kaleidoscope per prior reviewer notes) may still fail — only block on NEW failures.

- [ ] **Step 2: TypeScript strict pass**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 3: Catch any other `openPreviewAction` or direct `selectedPreviewFileAtom` setters**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/feat-preview-panel-multi-tab
grep -rn "openPreviewAction\|useSetAtom(selectedPreviewFileAtom\|set(selectedPreviewFileAtom" ui/src --include="*.tsx" --include="*.ts" 2>&1 | grep -v "preview-panel-atoms" | grep -v "preview-panel-atoms.test"
```

Expected: ONLY the comment/doc-string references in `preview-editor-atoms.ts`, `useDirtyBuffer.ts`, `FilePathChip.tsx` (doc-comment mentioning the action by name). No actual call sites should remain.

If you find a real call site (caller of openPreviewAction or direct setter on selectedPreviewFileAtom), migrate it on the spot: `set(openPreviewTabAction, { target, source })` with `'manual'` for user-initiated paths and `'agent'` for agent-emitted paths.

- [ ] **Step 4: Manual smoke test checklist (for the reviewer to run in dev build)**

Create a brief manual test note in the commit message of this task — no file change, just a checklist for whoever runs the dev build:

```
Manual verification:
  □ Open a chat session, click a FilePathChip in a message → tab appears + activates + panel opens
  □ Click another FilePathChip for a different file → second tab appears RIGHT of the first
  □ Click the first FilePathChip again → focuses existing tab (no duplicate)
  □ Have agent write a file (e.g. "write a hello.md") → tab inserts LEFT of manual tabs, with ✨ marker
  □ Click X on the active tab → activates right neighbor; if last tab, panel closes
  □ Middle-click a tab → closes it
  □ Switch workspace → all tabs disappear, panel collapses
  □ Open a file, edit it (dirty buffer), click X on its tab → confirm dialog appears
```

- [ ] **Step 5: Commit verification note (optional — can also skip this commit if Step 1-3 all clean)**

If you ran into ANY migration miss in Step 3, create a final commit fixing them and report. If everything was clean, no Step 5 commit needed — just report DONE to the controller.

---

## Self-review

**Spec coverage** — each spec section maps to a task:

| Spec section | Task |
|---|---|
| §Data model (atoms + interfaces) | Task 1 |
| §Actions (openPreviewTabAction, closePreviewTabAction, clearAllPreviewTabsAction, wrapper) | Task 1 |
| §Selected-file derived | Task 1 |
| §UI components (PreviewTabBar, PreviewTabItem) | Task 2 |
| §PreviewPanel.tsx mount | Task 3 |
| §Integration: agent listener | Task 4 |
| §Integration: manual click (FilePathChip, SidePanel) | Task 5 |
| §Integration: workspace-shell reset | Task 6 |
| §Testing (unit + RTL + manual) | Tasks 1, 2, 7 |
| §Edge cases (last-tab-closes-panel, neighbor activation, dirty prompt on close) | Task 1 (impl + tests) |
| §Risks (other callers, mountId duplication, monaco flicker) | Task 7 verification |

**Placeholder scan** — no TBD/TODO/FIXME left in the plan; every code block is complete.

**Type consistency** — `PreviewTabItem`, `PreviewTabSource`, `previewTabKey`, `openPreviewTabAction`, `closePreviewTabAction`, `clearAllPreviewTabsAction`, `activePreviewTabKeyAtom`, `previewTabsAtom` — names used consistently across all 7 tasks.

**PR shape:** 7 bisectable commits, target `main`. Each commit independently ships some user-observable progress (Tasks 1-3 wire the foundation; Task 4 makes agent files appear in the tab bar; Tasks 5-6 fix the existing manual paths to also surface as tabs).

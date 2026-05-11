# Workspace Phase 4a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [`docs/superpowers/specs/2026-05-11-workspace-phase4a-design.md`](../specs/2026-05-11-workspace-phase4a-design.md)

**Goal:** Add ⌘ 1..9 / Ctrl+1..9 workspace shortcuts, an optional folder picker in WorkspaceCreateDialog, and a `TabBarWorkspaceChip` that shows the active workspace at the TabBar left edge with a dropdown switcher.

**Architecture:** Three frontend-only features over the existing Phase 2 workspace data layer. Shortcuts plug into the existing `useShortcuts` hook + `shortcut-defaults.ts` registry. CreateDialog gains an `overridePath` state and slug preview; submit passes that path through the existing `createWorkspace(name, path?, icon?)` IPC. The chip is a new component mounted as TabBar's leftmost child, with a Radix `DropdownMenu` listing workspaces (sort_order ASC) and their `⌘ N` hints. No backend changes; no migrations.

**Tech Stack:** React 18 + Jotai (`workspacesAtom`, `selectWorkspaceAtom`) + Radix `DropdownMenu` + Vitest. Reuses existing `useShortcuts` hook and `openFolderDialog` bridge.

---

## File Structure

**Modify (TS):**
- `ui/src/lib/shortcut-defaults.ts` — append 9 entries to `SHORTCUT_DEFINITIONS` for `switch-workspace-1..9` (Task 1)
- `ui/src/components/shortcuts/GlobalShortcuts.tsx` — add 9 handler entries that call `selectWorkspaceAtom` (Task 1)
- `ui/src/components/workspace/WorkspaceCreateDialog.tsx` — add folder-picker row + override state + slug preview (Task 2)
- `ui/src/components/tabs/TabBar.tsx:198-228` — mount `<TabBarWorkspaceChip />` as the leftmost child of the `<TabBarInner>` flex container (Task 3)

**Create (TS):**
- `ui/src/components/tabs/TabBarWorkspaceChip.tsx` — the chip component + Radix `DropdownMenu` switcher (Task 3)
- `ui/src/components/tabs/TabBarWorkspaceChip.test.tsx` — 6 cases (Task 3)
- `ui/src/components/workspace/WorkspaceCreateDialog.test.tsx` — 4 cases (Task 2)
- `ui/src/components/shortcuts/GlobalShortcuts.test.tsx` — 3 cases (Task 1)

---

## Conventions for this plan

- Run from repo root `/Users/ryanliu/Documents/uclaw` unless noted.
- Branch is already `claude/workspace-phase4` (created off main `a5a24a4`; spec committed at `136dba5`).
- Each task ends with a commit. Commit messages are pre-written — copy verbatim.
- After each commit: `cd ui && npx tsc --noEmit 2>&1 | head` should be empty. Vitest scoped to the new test files should pass.
- Build stays green at every commit. If a step can't complete without breaking the build, **stop and escalate** rather than push a broken state.
- TDD where there's logic to test (shortcuts handler math, CreateDialog state). Pure presentational additions (the chip's render) are tested via interaction tests, not snapshot tests.

---

### Task 1: ⌘ 1..9 / Ctrl+1..9 workspace switching shortcuts

Spec §4.2. Nine new entries in `SHORTCUT_DEFINITIONS`, nine handler entries in `GlobalShortcuts.tsx` that look up `workspaces[i]` and call `selectWorkspaceAtom`. Out-of-range = silent no-op.

**Files:**
- Modify: `ui/src/lib/shortcut-defaults.ts` (append definitions after the `导航` group)
- Modify: `ui/src/components/shortcuts/GlobalShortcuts.tsx` (append handler block)
- Create: `ui/src/components/shortcuts/GlobalShortcuts.test.tsx`

- [ ] **Step 1: Write the failing test for the shortcuts**

Create `ui/src/components/shortcuts/GlobalShortcuts.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { GlobalShortcuts } from './GlobalShortcuts'
import { workspacesAtom, selectWorkspaceAtom } from '@/atoms/workspace'
import type { WorkspaceInfo } from '@/atoms/workspace'

function makeWs(id: string, name: string, sortOrder: number): WorkspaceInfo {
  return {
    id,
    name,
    icon: '📁',
    path: `/tmp/${id}`,
    attachedDirs: [],
    sortOrder,
    createdAt: '2026-05-11T00:00:00Z',
    updatedAt: '2026-05-11T00:00:00Z',
  }
}

function fireCmdDigit(digit: number) {
  const event = new KeyboardEvent('keydown', {
    key: String(digit),
    metaKey: true,   // jsdom navigator.userAgent doesn't include 'Mac', but
                     // the hook reads navigator at module load so we cover
                     // both branches by setting both modifiers below.
    ctrlKey: true,
    bubbles: true,
  })
  window.dispatchEvent(event)
}

describe('GlobalShortcuts: workspace shortcuts', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('Cmd+3 selects workspaces[2]', () => {
    const store = createStore()
    const wsList = [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ]
    store.set(workspacesAtom, wsList)
    const selectSpy = vi.fn()
    // Override the write atom by re-setting it would touch internals;
    // instead spy on the atom's effect by checking activeWorkspaceIdAtom
    // after the keypress.
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    // selectWorkspaceAtom is a write-only atom; we can't easily spy.
    // Verify by reading active id after the keypress (the bridge call
    // is mocked at module level — see vi.mock below).
    fireCmdDigit(3)
    // The bridge mock records the setActiveWorkspaceId call; check it.
    expect(selectSpy).not.toHaveBeenCalled() // placeholder — replaced below
    void selectWorkspaceAtom
  })

  it('out-of-range Cmd+5 is a no-op when only 3 workspaces exist', () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    expect(() => fireCmdDigit(5)).not.toThrow()
  })

  it('Cmd+1 with empty workspace list is a no-op', () => {
    const store = createStore()
    store.set(workspacesAtom, [])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    expect(() => fireCmdDigit(1)).not.toThrow()
  })
})
```

Note: the first test is intentionally written so the `selectSpy` placeholder will fail. We'll switch to mocking the bridge in Step 2 once we see the design of the test fixture.

Actually — **replace the first test** with a version that mocks `@/lib/tauri-bridge` and verifies the call:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render } from '@testing-library/react'
import { GlobalShortcuts } from './GlobalShortcuts'
import { workspacesAtom } from '@/atoms/workspace'
import type { WorkspaceInfo } from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  updateWorkspace: vi.fn(),
  reorderWorkspaces: vi.fn(),
}))

function makeWs(id: string, name: string, sortOrder: number): WorkspaceInfo {
  return {
    id, name, icon: '📁',
    path: `/tmp/${id}`,
    attachedDirs: [],
    sortOrder,
    createdAt: '2026-05-11T00:00:00Z',
    updatedAt: '2026-05-11T00:00:00Z',
  }
}

function fireDigit(digit: number) {
  window.dispatchEvent(new KeyboardEvent('keydown', {
    key: String(digit), metaKey: true, ctrlKey: true, bubbles: true,
  }))
}

describe('GlobalShortcuts: workspace shortcuts', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('Cmd+3 calls setActiveWorkspaceId for workspaces[2]', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(3)
    expect(setActiveWorkspaceId).toHaveBeenCalledWith('w3')
  })

  it('out-of-range Cmd+5 is a no-op when only 3 workspaces exist', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(5)
    expect(setActiveWorkspaceId).not.toHaveBeenCalled()
  })

  it('Cmd+1 with empty workspace list is a no-op', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [])
    render(
      <Provider store={store}>
        <GlobalShortcuts />
      </Provider>
    )
    fireDigit(1)
    expect(setActiveWorkspaceId).not.toHaveBeenCalled()
  })
})
```

(Use this version — discard the first attempt.)

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
cd ui && npm test -- --run GlobalShortcuts 2>&1 | tail -15
```

Expected: 3 failures because `switch-workspace-N` definitions don't exist yet, so no keypress matches.

- [ ] **Step 3: Append 9 shortcut definitions**

Open `ui/src/lib/shortcut-defaults.ts`. Find the existing `导航` group (ends around the `open-shortcuts` entry near line 84). After the closing `}` of that entry, insert:

```ts
  // ─── 工作区切换 ───
  ...Array.from({ length: 9 }, (_, i) => ({
    id: `switch-workspace-${i + 1}`,
    label: `切换到第 ${i + 1} 个工作区`,
    group: '导航',
    mac: `Cmd+${i + 1}`,
    win: `Ctrl+${i + 1}`,
  })),
```

The spread sits inside the `SHORTCUT_DEFINITIONS` array literal — make sure it's a sibling of the surrounding object entries.

- [ ] **Step 4: Append the handler block to GlobalShortcuts**

Open `ui/src/components/shortcuts/GlobalShortcuts.tsx`. Replace its current body so the existing handlers plus the new workspace handlers all flow through one `useShortcuts` call:

```tsx
import { useAtomValue, useSetAtom } from 'jotai'
import { useShortcuts } from '@/hooks/useShortcut'
import { sidebarCollapsedAtom } from '@/atoms/tab-atoms'
import { workspacesAtom, selectWorkspaceAtom } from '@/atoms/workspace'

export function GlobalShortcuts(): null {
  const setSidebarCollapsed = useSetAtom(sidebarCollapsedAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)

  const workspaceShortcuts = Array.from({ length: 9 }, (_, i) => ({
    id: `switch-workspace-${i + 1}`,
    handler: () => {
      const ws = workspaces[i]
      if (!ws) return
      void selectWorkspace(ws.id)
    },
  }))

  useShortcuts([
    {
      id: 'new-chat',
      handler: () => { console.log('[GlobalShortcuts] new-chat triggered') },
    },
    {
      id: 'new-agent-session',
      handler: () => { console.log('[GlobalShortcuts] new-agent-session triggered') },
    },
    {
      id: 'toggle-sidebar',
      handler: () => { setSidebarCollapsed((prev) => !prev) },
    },
    {
      id: 'open-settings',
      handler: () => { console.log('[GlobalShortcuts] open-settings triggered') },
    },
    {
      id: 'global-search',
      handler: () => { console.log('[GlobalShortcuts] global-search triggered') },
    },
    {
      id: 'focus-input',
      handler: () => {
        const input = document.querySelector<HTMLTextAreaElement>('textarea[data-input-main]')
        input?.focus()
      },
    },
    ...workspaceShortcuts,
  ])

  return null
}
```

(Preserve the existing placeholder handlers; we're only adding the 9 new entries.)

- [ ] **Step 5: Run the tests to confirm they pass**

```bash
cd ui && npm test -- --run GlobalShortcuts 2>&1 | tail -15
```

Expected: 3 passed.

- [ ] **Step 6: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: empty.

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/lib/shortcut-defaults.ts ui/src/components/shortcuts/GlobalShortcuts.tsx ui/src/components/shortcuts/GlobalShortcuts.test.tsx
git commit -m "feat(shortcuts): Cmd+1..9 / Ctrl+1..9 for workspace switching

9 new entries in SHORTCUT_DEFINITIONS (导航 group), one handler per
slot in GlobalShortcuts that looks up workspaces[i] and calls
selectWorkspaceAtom. Out-of-range presses are silent no-ops. Ordering
follows workspacesAtom which is sort_order ASC (Phase 2).

3 Vitest cases: in-range selects correct workspace, out-of-range is
no-op, empty list is no-op."
```

---

### Task 2: WorkspaceCreateDialog folder-picker + slug preview

Spec §4.1. Dialog gains an `overridePath` state, a slug preview, and "选择其他位置..." / "清除" buttons. Submit passes `overridePath ?? undefined` to `createWorkspace`.

**Files:**
- Modify: `ui/src/components/workspace/WorkspaceCreateDialog.tsx`
- Create: `ui/src/components/workspace/WorkspaceCreateDialog.test.tsx`

- [ ] **Step 1: Write the failing tests**

Create `ui/src/components/workspace/WorkspaceCreateDialog.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'

vi.mock('@/lib/tauri-bridge', async () => {
  return {
    createWorkspace: vi.fn().mockResolvedValue({ id: 'new-id', name: 'x', icon: '📁' }),
    openFolderDialog: vi.fn().mockResolvedValue({ path: '/custom/picked', name: 'picked' }),
  }
})

describe('WorkspaceCreateDialog', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('preview path follows slugified name', () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'My Project!' } })
    expect(screen.getByText(/~\/Documents\/workground\/my-project/)).toBeInTheDocument()
  })

  it('"选择其他位置..." overrides the preview path', async () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'thing' } })
    expect(screen.getByText(/workground\/thing/)).toBeInTheDocument()
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => {
      expect(screen.getByText('/custom/picked')).toBeInTheDocument()
    })
  })

  it('"清除" reverts the preview to slug', async () => {
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    const input = screen.getByPlaceholderText('Workspace name') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'thing' } })
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => expect(screen.getByText('/custom/picked')).toBeInTheDocument())
    fireEvent.click(screen.getByText('清除'))
    expect(screen.getByText(/workground\/thing/)).toBeInTheDocument()
  })

  it('Create call passes overridePath when set, undefined when not', async () => {
    const { createWorkspace } = await import('@/lib/tauri-bridge')
    // First: no override → passes undefined
    const { unmount } = renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    fireEvent.change(screen.getByPlaceholderText('Workspace name'), { target: { value: 'plain' } })
    fireEvent.click(screen.getByText('Create'))
    await waitFor(() => {
      expect(createWorkspace).toHaveBeenCalledWith('plain', undefined, '📁')
    })
    unmount()
    vi.clearAllMocks()
    // Second: with override → passes picked path
    renderWithProviders(
      <WorkspaceCreateDialog open onClose={() => {}} onCreated={() => {}} />
    )
    fireEvent.change(screen.getByPlaceholderText('Workspace name'), { target: { value: 'plain' } })
    fireEvent.click(screen.getByText('选择其他位置...'))
    await waitFor(() => expect(screen.getByText('/custom/picked')).toBeInTheDocument())
    fireEvent.click(screen.getByText('Create'))
    await waitFor(() => {
      expect(createWorkspace).toHaveBeenCalledWith('plain', '/custom/picked', '📁')
    })
  })
})
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd ui && npm test -- --run WorkspaceCreateDialog 2>&1 | tail -15
```

Expected: 4 failures (preview row doesn't exist, "选择其他位置..." button doesn't exist).

- [ ] **Step 3: Rewrite WorkspaceCreateDialog**

Replace the entire content of `ui/src/components/workspace/WorkspaceCreateDialog.tsx`:

```tsx
import * as React from 'react'
import { FolderOpen } from 'lucide-react'
import { toast } from 'sonner'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import * as bridge from '@/lib/tauri-bridge'

interface WorkspaceCreateDialogProps {
  open: boolean
  onClose: () => void
  onCreated: (ws: { id: string; name: string; icon: string }) => void
}

const EMOJI_CHOICES = ['📁', '💼', '🚀', '🔬', '✍️', '🎯', '🏠', '⚙️']

/**
 * Best-effort client-side slug preview matching the backend's slugify():
 * ASCII lowercase, non-alphanumeric → '-', collapse repeats, trim, max 32.
 * Informational only — the backend's compute_workspace_dir is authoritative.
 */
function slugifyPreview(name: string): string {
  return name.toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32)
}

export function WorkspaceCreateDialog({
  open,
  onClose,
  onCreated,
}: WorkspaceCreateDialogProps): React.ReactElement {
  const [name, setName] = React.useState('')
  const [icon, setIcon] = React.useState('📁')
  const [overridePath, setOverridePath] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  // Reset all dialog state on close.
  const resetAndClose = React.useCallback(() => {
    setName('')
    setIcon('📁')
    setOverridePath(null)
    onClose()
  }, [onClose])

  const computedPath = React.useMemo(() => {
    if (overridePath) return overridePath
    const slug = slugifyPreview(name)
    return slug ? `~/Documents/workground/${slug}` : '~/Documents/workground/...'
  }, [name, overridePath])

  const handlePickFolder = async () => {
    try {
      const picked = await bridge.openFolderDialog()
      if (picked) setOverridePath(picked.path)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`选择文件夹失败: ${msg}`)
    }
  }

  const handleCreate = async () => {
    if (!name.trim()) return
    setLoading(true)
    try {
      const ws = await bridge.createWorkspace(name.trim(), overridePath ?? undefined, icon)
      onCreated(ws)
      resetAndClose()
    } catch (e) {
      console.error('[workspace] create failed', e)
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && resetAndClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>New Workspace</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3 py-2">
          <div className="flex gap-2 flex-wrap">
            {EMOJI_CHOICES.map((e) => (
              <button
                key={e}
                onClick={() => setIcon(e)}
                className={`text-xl p-1 rounded ${icon === e ? 'ring-2 ring-primary' : ''}`}
              >
                {e}
              </button>
            ))}
          </div>
          <Input
            placeholder="Workspace name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
            autoFocus
          />
          <div className="flex flex-col gap-1.5">
            <label className="text-xs text-muted-foreground">目录</label>
            <div className="font-mono text-xs text-muted-foreground/80 truncate" title={computedPath}>
              {computedPath}
            </div>
            <div className="flex items-center gap-2 mt-1">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handlePickFolder}
                className="text-xs h-7 gap-1.5"
              >
                <FolderOpen className="size-3" />
                选择其他位置...
              </Button>
              {overridePath && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setOverridePath(null)}
                  className="text-xs h-7 text-muted-foreground hover:text-foreground"
                >
                  清除
                </Button>
              )}
            </div>
            {!overridePath && slugifyPreview(name) === '' && name.trim() && (
              <p className="text-[10px] text-muted-foreground/70 mt-0.5">
                名称只含非 ASCII 字符,将自动生成 workspace-xxx 目录。
              </p>
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={resetAndClose}>Cancel</Button>
          <Button onClick={handleCreate} disabled={!name.trim() || loading}>
            {loading ? 'Creating…' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

Key changes from the original:
- New `overridePath` state, reset on close
- New `slugifyPreview` helper
- New `computedPath` memo
- New `handlePickFolder` async handler that toasts on error
- New 目录 row with monospace preview + two buttons
- `handleCreate` now passes `overridePath ?? undefined`
- Cancel button now uses `resetAndClose` so it clears `overridePath`
- Added the CJK-only-name hint

- [ ] **Step 4: Run the tests to confirm GREEN**

```bash
cd ui && npm test -- --run WorkspaceCreateDialog 2>&1 | tail -15
```

Expected: 4 passed.

- [ ] **Step 5: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: empty.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/workspace/WorkspaceCreateDialog.tsx ui/src/components/workspace/WorkspaceCreateDialog.test.tsx
git commit -m "feat(workspace): folder picker + slug preview in CreateDialog

CreateDialog gains an optional folder-picker override. Default behavior
(auto-slug → ~/Documents/workground/<slug>) unchanged so existing
one-click flow still works.

New state:
- overridePath: string | null
- computedPath: useMemo deriving preview from slug(name) or overridePath
- handlePickFolder: calls openFolderDialog, sets override

The slug preview is informational only — submit passes
overridePath ?? undefined so the backend's compute_workspace_dir
remains the source of truth for auto-derived paths.

Cancel and dialog-close reset all state including overridePath. CJK-only
name shows a hint that the backend will fall back to workspace-xxx.

4 Vitest cases: slug preview reactivity, override path applied, clear
reverts to slug, submit passes correct path."
```

---

### Task 3: TabBarWorkspaceChip + dropdown switcher

Spec §4.3. New component mounted as the leftmost child of the TabBarInner flex container. Click opens a Radix `DropdownMenu` listing workspaces with `⌘ N` hints and a "+ 新建工作区" footer.

**Files:**
- Create: `ui/src/components/tabs/TabBarWorkspaceChip.tsx`
- Create: `ui/src/components/tabs/TabBarWorkspaceChip.test.tsx`
- Modify: `ui/src/components/tabs/TabBar.tsx:204` (inject chip as first child of the flex)

- [ ] **Step 1: Write the failing tests**

Create `ui/src/components/tabs/TabBarWorkspaceChip.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { Provider, createStore } from 'jotai'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { TabBarWorkspaceChip } from './TabBarWorkspaceChip'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  type WorkspaceInfo,
} from '@/atoms/workspace'

vi.mock('@/lib/tauri-bridge', () => ({
  setActiveWorkspaceId: vi.fn().mockResolvedValue(undefined),
  listSpaces: vi.fn().mockResolvedValue([]),
  getActiveWorkspaceId: vi.fn().mockResolvedValue(null),
  createWorkspace: vi.fn().mockResolvedValue({ id: 'new', name: 'new', icon: '📁' }),
  openFolderDialog: vi.fn(),
}))

function makeWs(id: string, name: string, sortOrder: number, icon = '📁'): WorkspaceInfo {
  return {
    id, name, icon, path: `/tmp/${id}`, attachedDirs: [], sortOrder,
    createdAt: '2026-05-11T00:00:00Z', updatedAt: '2026-05-11T00:00:00Z',
  }
}

function renderWithStore(store: ReturnType<typeof createStore>) {
  return render(<Provider store={store}><TabBarWorkspaceChip /></Provider>)
}

describe('TabBarWorkspaceChip', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('renders active workspace name and emoji', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', '2222', 0, '📁')])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    expect(screen.getByText('2222')).toBeInTheDocument()
    expect(screen.getByText('📁')).toBeInTheDocument()
  })

  it('truncates workspace name longer than 12 chars', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'abcdefghijklmnopqrst', 0)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    expect(screen.getByText('abcdefghijkl…')).toBeInTheDocument()
  })

  it('returns null when there is no active workspace', () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one', 0)])
    store.set(activeWorkspaceIdAtom, null)
    const { container } = renderWithStore(store)
    expect(container.textContent).toBe('')
  })

  it('opens dropdown with all workspaces and shortcut hints for first 9', async () => {
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
      makeWs('w3', 'Third', 2),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    fireEvent.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => {
      expect(screen.getByText('First')).toBeInTheDocument()
      expect(screen.getByText('Second')).toBeInTheDocument()
      expect(screen.getByText('Third')).toBeInTheDocument()
      // Mac userAgent in jsdom is typically empty — match either prefix.
      const hint1 = screen.getByText(/^(?:⌘|Ctrl\+)1$/)
      expect(hint1).toBeInTheDocument()
    })
  })

  it('clicking a workspace item calls setActiveWorkspaceId', async () => {
    const { setActiveWorkspaceId } = await import('@/lib/tauri-bridge')
    const store = createStore()
    store.set(workspacesAtom, [
      makeWs('w1', 'First', 0),
      makeWs('w2', 'Second', 1),
    ])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    fireEvent.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => expect(screen.getByText('Second')).toBeInTheDocument())
    fireEvent.click(screen.getByText('Second'))
    await waitFor(() => {
      expect(setActiveWorkspaceId).toHaveBeenCalledWith('w2')
    })
  })

  it('"+ 新建工作区" opens the CreateDialog', async () => {
    const store = createStore()
    store.set(workspacesAtom, [makeWs('w1', 'one', 0)])
    store.set(activeWorkspaceIdAtom, 'w1')
    renderWithStore(store)
    fireEvent.click(screen.getByRole('button', { name: /工作区/ }))
    await waitFor(() => expect(screen.getByText('新建工作区')).toBeInTheDocument())
    fireEvent.click(screen.getByText('新建工作区'))
    await waitFor(() => {
      expect(screen.getByText('New Workspace')).toBeInTheDocument()
    })
  })
})
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd ui && npm test -- --run TabBarWorkspaceChip 2>&1 | tail -15
```

Expected: 6 failures (component doesn't exist).

- [ ] **Step 3: Create the chip component**

Create `ui/src/components/tabs/TabBarWorkspaceChip.tsx`:

```tsx
/**
 * TabBarWorkspaceChip — leftmost element of TabBar showing the active
 * workspace + a dropdown to switch between workspaces.
 *
 * Mounted by TabBar. Hidden when no active workspace. Dropdown lists
 * all workspaces (sort_order ASC) with their Cmd+N hints for the first
 * 9 entries; footer item opens WorkspaceCreateDialog.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { ChevronDown, Check, Plus } from 'lucide-react'
import {
  DropdownMenu, DropdownMenuTrigger, DropdownMenuContent,
  DropdownMenuItem, DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  selectWorkspaceAtom,
  refreshWorkspacesAtom,
} from '@/atoms/workspace'
import { WorkspaceCreateDialog } from '@/components/workspace/WorkspaceCreateDialog'
import { cn } from '@/lib/utils'

const isMac = typeof navigator !== 'undefined'
  && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)
const modPrefix = isMac ? '⌘' : 'Ctrl+'

const MAX_NAME_CHARS = 12

function truncateName(name: string): string {
  if (name.length <= MAX_NAME_CHARS) return name
  return `${name.slice(0, MAX_NAME_CHARS)}…`
}

export function TabBarWorkspaceChip(): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const refresh = useSetAtom(refreshWorkspacesAtom)
  const [createOpen, setCreateOpen] = React.useState(false)

  const active = workspaces.find((w) => w.id === activeId)
  if (!active) return null

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="titlebar-no-drag flex items-center gap-1 px-2 py-1 rounded-md
                       text-[12px] text-foreground/80 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors shrink-0"
            aria-label={`工作区: ${active.name}`}
            title={`工作区: ${active.name}`}
          >
            <span className="leading-none text-[13px]">{active.icon}</span>
            <span className="font-medium">{truncateName(active.name)}</span>
            <ChevronDown className="size-3 text-muted-foreground/60" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" sideOffset={4} className="w-56 z-[100]">
          {workspaces.map((w, i) => (
            <DropdownMenuItem
              key={w.id}
              onSelect={() => { void selectWorkspace(w.id) }}
              className="flex items-center gap-2 text-xs"
            >
              <Check
                className={cn(
                  'size-3.5 shrink-0',
                  w.id === activeId ? 'opacity-100' : 'opacity-0'
                )}
              />
              <span className="text-[13px] leading-none">{w.icon}</span>
              <span className="flex-1 truncate">{w.name}</span>
              {i < 9 && (
                <span className="text-[10px] text-muted-foreground/60 font-mono shrink-0">
                  {modPrefix}{i + 1}
                </span>
              )}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onSelect={() => setCreateOpen(true)}
            className="flex items-center gap-2 text-xs text-primary"
          >
            <Plus className="size-3.5" />
            新建工作区
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <WorkspaceCreateDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={async (ws) => {
          await refresh()
          void selectWorkspace(ws.id)
        }}
      />
    </>
  )
}
```

- [ ] **Step 4: Run tests to verify intermediate state**

```bash
cd ui && npm test -- --run TabBarWorkspaceChip 2>&1 | tail -15
```

Expected: 6 passed (component renders in isolation; mount in TabBar is a separate step).

- [ ] **Step 5: Mount the chip in TabBar**

Open `ui/src/components/tabs/TabBar.tsx`. Find the `<TabBarInner>` return JSX (around line 198). Replace the inner flex container block:

```tsx
      <div className="relative flex items-end flex-1 min-w-0 overflow-x-clip titlebar-no-drag">
        {tabs.map((tab) => (
          <TabBarItem
            ...
          />
        ))}
      </div>
```

with:

```tsx
      <div className="relative flex items-end flex-1 min-w-0 overflow-x-clip titlebar-no-drag">
        <div className="flex items-center px-1 py-1 shrink-0 self-stretch">
          <TabBarWorkspaceChip />
        </div>
        {tabs.map((tab) => (
          <TabBarItem
            ...
          />
        ))}
      </div>
```

Add the import at the top of the file:

```ts
import { TabBarWorkspaceChip } from './TabBarWorkspaceChip'
```

The wrapping div uses `self-stretch` + `flex items-center` so the chip vertically centers within the 34px tab strip, and `shrink-0` keeps it visible even when many tabs squeeze the layout.

- [ ] **Step 6: TS check + full UI test**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: TS empty. Vitest summary should show the 6 new chip cases + the existing tests; the pre-existing `message.test.tsx` thead failure (carried from main) is the only expected failure.

- [ ] **Step 7: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/tabs/TabBarWorkspaceChip.tsx ui/src/components/tabs/TabBarWorkspaceChip.test.tsx ui/src/components/tabs/TabBar.tsx
git commit -m "feat(tabs): TabBarWorkspaceChip + dropdown workspace switcher

Leftmost element of TabBar shows the active workspace's emoji and
truncated name (12 chars max + ellipsis) with a chevron hint. Click
opens a Radix DropdownMenu listing all workspaces in sort_order ASC
order, each with a ⌘N / Ctrl+N hint for the first 9 entries. Footer
entry '+ 新建工作区' opens the existing WorkspaceCreateDialog.

The chip returns null when no active workspace exists (defensive —
Phase 1 V16 backfills 'default' so this shouldn't happen in practice).

z-[100] on DropdownMenuContent clears the sidebar's z-[60] stacking
context (Phase 2-established pattern).

6 Vitest cases covering: name render, truncation, null-state hide,
dropdown population + shortcut hints, item click → selectWorkspace,
+ 新建工作区 → CreateDialog open."
```

---

## Self-Review

**Spec coverage:**
- §2 Goal 1 (folder picker) → Task 2 ✓
- §2 Goal 2 (⌘ 1..9 shortcuts) → Task 1 ✓
- §2 Goal 3 (TabBarWorkspaceChip) → Task 3 ✓
- §2 Goal 4 (no backend changes) → all three tasks are frontend-only ✓
- §2 Goal 5 (build green at every commit) → enforced in conventions ✓
- §2 Goal 6 (~10 Vitest cases): tally — Task 1: 3, Task 2: 4, Task 3: 6. Total 13. Exceeds estimate, fine.

**Placeholder scan:** searched for TBD / TODO / fill-in / "implement later" — none in plan body. The CJK-only-name placeholder hint at §4.1 of the spec is intentional UI text, not a planning placeholder.

**Type consistency:**
- `workspacesAtom` returns `WorkspaceInfo[]` — used identically in Tasks 1 and 3 fixtures.
- `selectWorkspaceAtom` is a write-only Jotai atom; called via `useSetAtom(selectWorkspaceAtom)` in both Tasks 1 (handler) and 3 (item click).
- `createWorkspace(name: string, path?: string, icon?: string)` — Task 2 passes `overridePath ?? undefined` matching the optional `path` param.
- `openFolderDialog()` returns `{ path: string; name: string } | null` — Task 2 destructures `picked.path`.
- `truncateName` helper exists only in Task 3; not referenced elsewhere.

No gaps found.

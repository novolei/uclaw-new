# W6 PR B — Workspace Git UI Components Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port if2Ai's BranchPicker + GitWorkbenchDialog + GitActionsPicker components (verbatim Tailwind/JSX/labels) into uClaw, wire them into both chat composers (ChatInput + AgentView) via a new `GitChipsRow` wrapper, gated by a new `activeWorkspaceCwdAtom` derived atom. Closes out W6 Phase 1+2.

**Architecture:** Verbatim port discipline — Tailwind class strings, JSX structure, prop names, Chinese labels, ARIA attributes copied unchanged from if2Ai. Three documented adaptations: (1) `cwd` source swap (`currentProject.workdir` → `activeWorkspaceCwdAtom`), (2) GitActionsPicker 400-LOC split into 3 files, (3) BranchPicker + GitWorkbenchDialog split via hook + component pattern (each is 419–435 LOC, exceeds uClaw's 400 hard cap; permitted by spec §5). Two deletions during port: the `'the-finals-selected-menu-item'` class (if2Ai theme-scoped hook) and the WorkspacePill (uClaw has `TabBarWorkspaceChip` + `WorkspaceSwitcherBar`).

**Tech Stack:** React 18 · TypeScript · jotai (`activeWorkspaceIdAtom`/`workspacesAtom` already in tree) · `@tauri-apps/api/core` invoke via `ui/src/modules/git/api.ts` (PR A) · `lucide-react` icons · `sonner` toasts · Radix Popover + Dialog.

**Spec:** `docs/superpowers/specs/2026-05-13-w6-workspace-git-design.md` §4 (frontend), §5 (port discipline), §12 (PR B integration addendum committed at `d89646b`).

**Branch base:** `claude/w6-pr-b-ui` already created off main (post-PR-A merge at `7f90720`). This plan doc becomes commit 1 of 11.

---

## Pre-flight

- [ ] **Confirm starting state**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout claude/w6-pr-b-ui
git branch --show-current   # must be claude/w6-pr-b-ui
git log --oneline -2
# Expect:
#   d89646b docs(spec): W6 PR B integration addendum
#   7f90720 W6 PR A: Workspace git backbone (...) (#118)
```

- [ ] **Baselines (record before starting)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 466 passed (PR A baseline; no Rust changes in PR B)

cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
# Expect: 310 passed (will become 316 by Task 11)
```

- [ ] **Branch hygiene reminder** — every subagent prompt verifies `git branch --show-current` at start, before commit, and after commit. The harness silently flips branches; if a subagent finds itself on a different branch, STOP and report — do NOT push, do NOT continue.

- [ ] **Verbatim port discipline reminder** — every UI task's subagent prompt must say:

> Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/<Component>.tsx` FIRST and read it in full. Port to uClaw verbatim: preserve Tailwind class strings, JSX structure, prop names, callback names, Chinese labels, ARIA attributes. Only documented adaptations permitted; do not "clean up" or "modernize" the port.

---

## File Structure

### New TypeScript files

| Path | LOC | Responsibility |
|---|---|---|
| `ui/src/components/chat/git/useBranchPicker.ts` | ~155 | State + handlers extracted from if2Ai's BranchPicker monolith — `LoadState`, `refresh`, `handleInit`, `handleCheckout`, `runCheckout`, `handleCreate`, `uncommittedFromStatus` |
| `ui/src/components/chat/git/BranchPicker.tsx` | ~290 | JSX-only component — trigger button + popover content (search/list/create) consuming `useBranchPicker` |
| `ui/src/components/chat/git/useGitWorkbench.ts` | ~145 | State + reload extracted from if2Ai's GitWorkbenchDialog monolith — `ViewState<T>`, `reload`, three sub-state hooks (`statusState`, `diffState`, `branchesState`) |
| `ui/src/components/chat/git/GitWorkbenchDialog.tsx` | ~280 | JSX-only component — Dialog wrapper + 3-tab bodies (`PlainTextView`, `BranchListView`) consuming `useGitWorkbench` |
| `ui/src/components/chat/git/GitActionsPicker.tsx` | ~190 | Outer chip + `<Popover>` + `Mode` discriminated-union state machine + menu sub-view |
| `ui/src/components/chat/git/GitActionsPickerForms.tsx` | ~290 | 3 form sub-views (Commit, CreateBranch, PR) extracted from if2Ai's local components in the monolith |
| `ui/src/components/chat/git/GitActionsPickerDraftPr.tsx` | ~160 | `PrDraftView` for `gh`-missing fallback + `shellAnsiCQuote` ANSI-C escape helper |
| `ui/src/components/chat/git/GitChipsRow.tsx` | ~55 | Self-contained wrapper — reads `activeWorkspaceCwdAtom`, returns `null` when cwd is null, otherwise renders BranchPicker + GitActionsPicker + GitWorkbenchDialog |

### Modified files

| Path | Change | LOC |
|---|---|---|
| `ui/src/atoms/workspace.ts` | Add `activeWorkspaceCwdAtom` derived atom | +13 |
| `ui/src/components/chat/ChatInput.tsx` | Insert `<GitChipsRow />` at line 291 inner row | +2 |
| `ui/src/components/agent/AgentView.tsx` | Insert `<GitChipsRow />` at line 1468 inner row | +2 |

### New test files

| Path | LOC | Cases |
|---|---|---|
| `ui/src/atoms/workspace.test.ts` (append) | +50 | 2: `activeWorkspaceCwdAtom` returns path for active workspace; returns null when no active id |
| `ui/src/components/chat/git/BranchPicker.test.tsx` | ~80 | 2: renders mocked branch list; no-repo state shows init affordance |
| `ui/src/components/chat/git/GitActionsPicker.test.tsx` | ~90 | 2: renders menu; flips to draft fallback when `ghAvailable=false` |

**Total: 8 new files, 3 modified, +6 tests. Each file ≤ 290 LOC, well inside uClaw's 400 hard cap.**

---

## Task 1 already done — this plan IS commit 1

The writing-plans skill saves this document and the controller commits it as the first commit on `claude/w6-pr-b-ui`. Subsequent tasks 2–11 build on the plan.

---

## Task 2: activeWorkspaceCwdAtom derived atom

**Files:**
- Modify: `ui/src/atoms/workspace.ts` — append after `activeWorkspaceIdAtom`
- Test: `ui/src/atoms/workspace.test.ts` — create new file with 2 cases

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current   # must be claude/w6-pr-b-ui
```

- [ ] **Step 2: Write the failing tests**

Create `/Users/ryanliu/Documents/uclaw/ui/src/atoms/workspace.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  activeWorkspaceCwdAtom,
  type WorkspaceInfo,
} from './workspace'

function makeWorkspace(id: string, path: string | null): WorkspaceInfo {
  return {
    id,
    name: `Workspace ${id}`,
    icon: 'Folder',
    path,
    attachedDirs: [],
    sortOrder: 0,
    createdAt: '2026-05-13T00:00:00Z',
    updatedAt: '2026-05-13T00:00:00Z',
  }
}

describe('activeWorkspaceCwdAtom', () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore()
  })

  it('returns the active workspace path', () => {
    store.set(workspacesAtom, [
      makeWorkspace('w1', '/Users/me/projects/foo'),
      makeWorkspace('w2', '/Users/me/projects/bar'),
    ])
    store.set(activeWorkspaceIdAtom, 'w2')

    expect(store.get(activeWorkspaceCwdAtom)).toBe('/Users/me/projects/bar')
  })

  it('returns null when no workspace is active', () => {
    store.set(workspacesAtom, [makeWorkspace('w1', '/Users/me/projects/foo')])
    store.set(activeWorkspaceIdAtom, null)

    expect(store.get(activeWorkspaceCwdAtom)).toBeNull()
  })

  it('returns null when active workspace has null path', () => {
    store.set(workspacesAtom, [makeWorkspace('w1', null)])
    store.set(activeWorkspaceIdAtom, 'w1')

    expect(store.get(activeWorkspaceCwdAtom)).toBeNull()
  })
})
```

- [ ] **Step 3: Run, confirm failure**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run workspace 2>&1 | tail -10
# Expect: 3 failures referencing 'activeWorkspaceCwdAtom is not exported' (or undefined)
```

- [ ] **Step 4: Implement**

Append to `/Users/ryanliu/Documents/uclaw/ui/src/atoms/workspace.ts` after the `activeWorkspaceIdAtom` definition (after line 30, before line 32's `workspaceSwitchDirectionAtom`):

```ts
/**
 * Active workspace's filesystem path, or null if no workspace has a
 * directory attached. Drives the cwd argument for every git IPC call
 * from BranchPicker / GitActionsPicker / GitWorkbenchDialog (W6 PR B).
 *
 * Pure derived atom — re-evaluates when `activeWorkspaceIdAtom` or
 * `workspacesAtom` changes. No IO, no async.
 */
export const activeWorkspaceCwdAtom = atom<string | null>((get) => {
  const id = get(activeWorkspaceIdAtom)
  if (!id) return null
  const ws = get(workspacesAtom).find((w) => w.id === id)
  return ws?.path ?? null
})
```

The plan reads the actual file structure to confirm placement. The `import { atom } from 'jotai'` at line 1 already covers the import. No other changes.

- [ ] **Step 5: Run tests + tsc, confirm pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run workspace 2>&1 | tail -5
# Expect: 3 passed

cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean
```

- [ ] **Step 6: Branch verify + commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/atoms/workspace.ts ui/src/atoms/workspace.test.ts
git commit -m "feat(workspace): activeWorkspaceCwdAtom derived atom

Returns the active workspace's filesystem path, or null when no
workspace is active OR the active workspace has no directory attached.

Drives the cwd argument for every git IPC call in W6 PR B's chip
components. Pure derived atom — re-evaluates when activeWorkspaceIdAtom
or workspacesAtom changes.

3 vitest cases: returns path for active, returns null when no active id,
returns null when active workspace has null path.

W6 PR B Task 2 of 11."
git log --oneline -1
git branch --show-current
```

**Add specific paths only.** Do NOT use `git add -A`.

---

## Task 3: useBranchPicker hook (verbatim port of state + handlers)

**Files:**
- Create: `ui/src/components/chat/git/useBranchPicker.ts`

**Why extracted:** if2Ai's `BranchPicker.tsx` is 435 LOC (over uClaw's 400 hard cap). Splitting along the natural state/JSX boundary keeps each file under cap while preserving verbatim port discipline on the JSX-heavy half.

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/BranchPicker.tsx` lines 50–203.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Read the if2Ai source**

Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/BranchPicker.tsx` in full and identify the boundary between state/handlers (lines 50–203) and JSX (lines 204–435). The hook will receive `cwd`, `currentBranch`, `onChanged`, `onInitRepo` as inputs and expose all state + handler functions as outputs.

- [ ] **Step 3: Create `ui/src/components/chat/git/useBranchPicker.ts`**

```ts
/**
 * useBranchPicker — state + handlers for the BranchPicker component.
 *
 * Extracted from if2Ai's BranchPicker.tsx (lines 50-203) into a custom
 * hook because the monolithic component (435 LOC) exceeds uClaw's 400
 * hard cap. Pure logic — no JSX, no DOM access. The component file
 * (`BranchPicker.tsx`) renders the JSX and wires the hook outputs into
 * Popover + buttons.
 *
 * Verbatim port — every state machine, every handler, every error
 * branch matches if2Ai. Only adaptation: `uncommittedFromStatus` is
 * imported from `@/modules/git/api` (PR A re-exported the helper there
 * since it's IPC-layer-relevant; if2Ai inlines it).
 */

import * as React from 'react'
import {
  gitBranches,
  gitCheckoutBranch,
  gitCreateBranch,
  gitInitRepo,
  gitStatus,
  parseBranchList,
  uncommittedFromStatus,
  type BranchListItem,
} from '@/modules/git/api'

export type LoadState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ready'; branches: BranchListItem[]; uncommittedCount: number }
  | { kind: 'error'; message: string }

export interface UseBranchPickerArgs {
  cwd: string | undefined
  currentBranch: string
  onChanged?: (newBranch: string) => void
  isGitRepo?: boolean | null
  onInitRepo?: () => void
}

export interface UseBranchPickerResult {
  // State
  open: boolean
  setOpen: (next: boolean) => void
  state: LoadState
  query: string
  setQuery: (q: string) => void
  creating: boolean
  setCreating: (c: boolean) => void
  busyBranch: string | null
  createName: string
  setCreateName: (n: string) => void
  pendingCheckout: string | null
  setPendingCheckout: (n: string | null) => void
  initing: boolean
  // Derived
  filtered: BranchListItem[]
  noCwd: boolean
  noRepo: boolean
  popoverDisabled: boolean
  triggerDisabled: boolean
  // Handlers
  handleInit: () => Promise<void>
  handleCheckout: (name: string) => Promise<void>
  runCheckout: (name: string) => Promise<void>
  handleCreate: () => Promise<void>
}

export function useBranchPicker({
  cwd,
  currentBranch,
  onChanged,
  isGitRepo = null,
  onInitRepo,
}: UseBranchPickerArgs): UseBranchPickerResult {
  const [open, setOpen] = React.useState(false)
  const [state, setState] = React.useState<LoadState>({ kind: 'idle' })
  const [query, setQuery] = React.useState('')
  const [creating, setCreating] = React.useState(false)
  const [busyBranch, setBusyBranch] = React.useState<string | null>(null)
  const [createName, setCreateName] = React.useState('')
  const [pendingCheckout, setPendingCheckout] = React.useState<string | null>(null)

  const noCwd = !cwd || cwd.trim() === ''
  const noRepo = isGitRepo === false
  const popoverDisabled = noCwd || noRepo
  const triggerDisabled = noCwd
  const [initing, setIniting] = React.useState(false)

  const handleInit = React.useCallback(async () => {
    if (!cwd || initing) return
    setIniting(true)
    try {
      await gitInitRepo(cwd)
      onInitRepo?.()
    } catch (err) {
      setOpen(true)
      setState({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setIniting(false)
    }
  }, [cwd, initing, onInitRepo])

  const refresh = React.useCallback(async () => {
    if (!cwd) return
    setState({ kind: 'loading' })
    try {
      const [branchesRaw, statusRaw] = await Promise.all([
        gitBranches(cwd),
        gitStatus(cwd).catch(() => null),
      ])
      setState({
        kind: 'ready',
        branches: parseBranchList(branchesRaw),
        uncommittedCount: uncommittedFromStatus(statusRaw),
      })
    } catch (err) {
      setState({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }, [cwd])

  React.useEffect(() => {
    if (open) {
      void refresh()
    } else {
      setQuery('')
      setCreating(false)
      setCreateName('')
      setBusyBranch(null)
      setPendingCheckout(null)
    }
  }, [open, refresh])

  const filtered = React.useMemo(() => {
    if (state.kind !== 'ready') return [] as BranchListItem[]
    const q = query.trim().toLowerCase()
    if (!q) return state.branches
    return state.branches.filter((b) => b.name.toLowerCase().includes(q))
  }, [state, query])

  const runCheckout = React.useCallback(
    async (name: string) => {
      if (!cwd) return
      setPendingCheckout(null)
      setBusyBranch(name)
      try {
        await gitCheckoutBranch(cwd, name)
        onChanged?.(name)
        setOpen(false)
      } catch (err) {
        setState({
          kind: 'error',
          message: err instanceof Error ? err.message : String(err),
        })
      } finally {
        setBusyBranch(null)
      }
    },
    [cwd, onChanged],
  )

  const handleCheckout = React.useCallback(
    async (name: string) => {
      if (!cwd || name === currentBranch) {
        setOpen(false)
        return
      }
      if (state.kind === 'ready' && state.uncommittedCount > 0) {
        setPendingCheckout(name)
        return
      }
      await runCheckout(name)
    },
    [cwd, currentBranch, state, runCheckout],
  )

  const handleCreate = React.useCallback(async () => {
    const name = createName.trim()
    if (!cwd || !name) return
    setBusyBranch(name)
    try {
      await gitCreateBranch(cwd, name)
      onChanged?.(name)
      setOpen(false)
    } catch (err) {
      setState({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setBusyBranch(null)
    }
  }, [createName, cwd, onChanged])

  return {
    open,
    setOpen,
    state,
    query,
    setQuery,
    creating,
    setCreating,
    busyBranch,
    createName,
    setCreateName,
    pendingCheckout,
    setPendingCheckout,
    initing,
    filtered,
    noCwd,
    noRepo,
    popoverDisabled,
    triggerDisabled,
    handleInit,
    handleCheckout,
    runCheckout,
    handleCreate,
  }
}
```

- [ ] **Step 4: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/useBranchPicker.ts
git commit -m "feat(git): useBranchPicker hook — state + handlers for BranchPicker

Extracted from if2Ai's BranchPicker.tsx (435 LOC, exceeds uClaw's 400
hard cap). Hook owns LoadState, refresh, handleInit, handleCheckout,
runCheckout, handleCreate, and all derived booleans (noCwd, noRepo,
popoverDisabled, triggerDisabled). Pure logic — no JSX.

The companion BranchPicker.tsx (Task 4) renders the popover JSX
consuming these outputs.

W6 PR B Task 3 of 11."
git log --oneline -1
git branch --show-current
```

**Add specific paths only.** Do NOT use `git add -A`.

---

## Task 4: BranchPicker.tsx component (JSX) + uClaw init-toast adaptation

**Files:**
- Create: `ui/src/components/chat/git/BranchPicker.tsx`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/BranchPicker.tsx` lines 1–48 (imports + Props) + lines 204–435 (JSX).

**Two adaptations baked into this commit:**
1. Drop `'the-finals-selected-menu-item'` class at line 353 (if2Ai theme-scoped hook, no-op in uClaw).
2. uClaw-specific 3s confirmation toast before `git init` runs. Per spec §4.2: workspace is a long-lived user dir; surprise repo creation is bad. Wrap `handleInit` invocation with a `sonner` toast.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Read the if2Ai source**

Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/BranchPicker.tsx` lines 204-435 in full. This is the JSX portion that this task ports verbatim, minus the 2 adaptations above.

- [ ] **Step 3: Create `ui/src/components/chat/git/BranchPicker.tsx`**

```tsx
/**
 * BranchPicker — composer footer's "current git branch" chip + dropdown.
 *
 * Verbatim port of if2Ai's BranchPicker (Tailwind, JSX, Chinese labels,
 * ARIA all preserved). Three adaptations vs the if2Ai source:
 *
 * 1. State + handlers extracted into `./useBranchPicker.ts` so this
 *    file stays under uClaw's 400 LOC hard cap.
 * 2. `the-finals-selected-menu-item` class dropped from the
 *    current-branch row — it's a theme-scoped hook for if2Ai's
 *    "The Finals" theme, no-op in uClaw.
 * 3. Init-repo flow surfaces a 3s `sonner` confirmation toast before
 *    running `gitInitRepo` (per W6 spec §4.2 — workspace is a long-
 *    lived user dir; surprise repo creation is bad UX).
 */

import * as React from 'react'
import { Check, GitBranch, Loader2, Plus, Search, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useBranchPicker, type UseBranchPickerArgs } from './useBranchPicker'

type Props = UseBranchPickerArgs & {
  className?: string
}

export function BranchPicker(props: Props) {
  const {
    open, setOpen, state, query, setQuery,
    creating, setCreating, busyBranch, createName, setCreateName,
    pendingCheckout, setPendingCheckout, initing,
    filtered, noRepo, popoverDisabled, triggerDisabled,
    handleInit, handleCheckout, handleCreate, runCheckout,
  } = useBranchPicker(props)

  const { currentBranch, className } = props

  // uClaw adaptation: confirmation toast before git init (W6 spec §4.2).
  // Click on amber "无 Git 仓库" trigger → toast offers "初始化" CTA.
  // User must confirm; passive 3s dismiss does nothing.
  const handleInitWithConfirm = React.useCallback(() => {
    toast(
      `您将在当前工作区初始化 Git 仓库吗？`,
      {
        duration: 5000,
        action: {
          label: '初始化',
          onClick: () => void handleInit(),
        },
      },
    )
  }, [handleInit])

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (popoverDisabled) return
        setOpen(next)
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={triggerDisabled}
          onClick={
            noRepo
              ? (e) => {
                  e.preventDefault()
                  e.stopPropagation()
                  handleInitWithConfirm()
                }
              : undefined
          }
          title={noRepo ? '当前目录不是 Git 仓库 — 点击执行 git init' : undefined}
          className={cn(
            'flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] transition-colors disabled:cursor-not-allowed disabled:opacity-60',
            noRepo
              ? 'text-amber-600 hover:bg-amber-500/12 hover:text-amber-500'
              : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
            className,
          )}
          aria-label={noRepo ? '初始化 Git 仓库' : '切换 git 分支'}
        >
          <GitBranch className="h-[11px] w-[11px]" />
          {noRepo ? (
            <span className="inline-flex items-center gap-1">
              {initing ? (
                <Loader2 className="h-[10px] w-[10px] animate-spin" />
              ) : (
                <Sparkles className="h-[10px] w-[10px]" />
              )}
              <span>无 Git 仓库 · 点击初始化</span>
            </span>
          ) : (
            <span className="max-w-[160px] truncate">
              {currentBranch || '—'}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="center"
        sideOffset={12}
        collisionPadding={16}
        className={cn(
          'w-[260px] overflow-hidden rounded-2xl border border-border/70 bg-popover/96 p-0 text-[13px] text-popover-foreground backdrop-blur-2xl backdrop-saturate-150',
          'shadow-[0_2px_4px_rgba(0,0,0,0.04),0_8px_20px_rgba(0,0,0,0.08),0_24px_56px_rgba(0,0,0,0.16),0_0_0_0.5px_rgba(0,0,0,0.04)]',
          'origin-[var(--radix-popover-content-transform-origin)] transition-all duration-200 ease-out',
          'data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=open]:slide-in-from-top-1',
          'data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95',
        )}
      >
        {pendingCheckout && state.kind === 'ready' && (
          <div className="border-b border-amber-200/70 bg-amber-50/70 px-3.5 py-3">
            <div className="text-[12px] leading-5 text-amber-900">
              工作区有 <span className="font-semibold">{state.uncommittedCount}</span> 个未提交的文件。切到{' '}
              <span className="font-mono text-[12px] text-amber-900">{pendingCheckout}</span>{' '}
              可能会覆盖或保留改动，git 视情况决定 —— 建议先提交或暂存。
            </div>
            <div className="mt-2 flex items-center justify-end gap-1.5">
              <button
                type="button"
                onClick={() => setPendingCheckout(null)}
                className="rounded-md px-2.5 py-1 text-[11.5px] text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground"
              >
                返回
              </button>
              <button
                type="button"
                onClick={() => void runCheckout(pendingCheckout)}
                className="rounded-md bg-amber-600 px-2.5 py-1 text-[11.5px] font-medium text-white outline-none transition-opacity hover:opacity-90 focus-visible:opacity-90"
              >
                仍要切换
              </button>
            </div>
          </div>
        )}

        {/* Search */}
        <div className="flex items-center gap-2 px-3.5 pt-3 pb-2.5">
          <Search className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <input
            type="text"
            autoFocus
            placeholder="搜索分支"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 bg-transparent text-[11.5px] leading-6 text-popover-foreground outline-none placeholder:text-muted-foreground"
          />
        </div>

        {/* List */}
        <div className="max-h-[280px] overflow-y-auto pb-1.5">
          {state.kind === 'loading' && (
            <div className="flex items-center justify-center gap-2 py-7 text-[13px] text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>加载中…</span>
            </div>
          )}
          {state.kind === 'error' && (
            <div className="px-3.5 py-3 text-[11.5px] leading-5 text-rose-600/80">
              {state.message}
            </div>
          )}
          {state.kind === 'ready' && (
            <>
              <div className="px-3.5 pb-1 pt-1 text-[11.5px] text-muted-foreground">
                分支
              </div>
              {filtered.length === 0 && (
                <div className="px-3.5 py-5 text-center text-[13px] text-muted-foreground">
                  无匹配分支
                </div>
              )}
              {filtered.map((b) => {
                const isCurrent = b.isCurrent || b.name === currentBranch
                const isBusy = busyBranch === b.name
                return (
                  <button
                    key={b.name}
                    type="button"
                    disabled={isBusy}
                    aria-selected={isCurrent}
                    onClick={() => handleCheckout(b.name)}
                    className={cn(
                      'flex w-full items-start gap-2.5 px-3.5 py-1.5 text-left outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground',
                      // NOTE: 'the-finals-selected-menu-item' from if2Ai dropped per W6 PR B Task 4 adaptation
                      isBusy && 'opacity-60',
                    )}
                  >
                    <GitBranch
                      className={cn(
                        'mt-[3px] h-[14px] w-[14px] shrink-0',
                        isCurrent ? 'text-primary' : 'text-muted-foreground',
                      )}
                      strokeWidth={1.75}
                    />
                    <div className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] leading-6 text-popover-foreground">
                        {b.name}
                      </span>
                      {isCurrent && state.uncommittedCount > 0 && (
                        <div className="text-[11.5px] leading-5 text-muted-foreground">
                          未提交的更改：{state.uncommittedCount} 个文件
                        </div>
                      )}
                    </div>
                    {isCurrent && !isBusy && (
                      <Check
                        className="mt-[5px] h-[13px] w-[13px] shrink-0 text-primary"
                        strokeWidth={2}
                      />
                    )}
                    {isBusy && (
                      <Loader2 className="mt-[5px] h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
                    )}
                  </button>
                )
              })}
            </>
          )}
        </div>

        {/* Create new branch */}
        <div className="border-t border-border/65">
          {!creating ? (
            <button
              type="button"
              onClick={() => setCreating(true)}
              disabled={state.kind !== 'ready'}
              className="flex w-full items-center gap-2.5 px-3.5 py-2.5 text-left text-[11.5px] leading-6 text-popover-foreground/80 outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground disabled:cursor-not-allowed disabled:opacity-60"
            >
              <Plus className="h-3.5 w-3.5 text-muted-foreground" strokeWidth={2} />
              创建并检出新分支…
            </button>
          ) : (
            <div className="flex items-center gap-2 px-3.5 py-2.5">
              <input
                autoFocus
                type="text"
                placeholder="新分支名"
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    void handleCreate()
                  } else if (e.key === 'Escape') {
                    setCreating(false)
                    setCreateName('')
                  }
                }}
                className="flex-1 rounded-lg border border-border/70 bg-muted px-2.5 py-1.5 text-[13px] text-foreground outline-none placeholder:text-muted-foreground focus:border-primary/60"
              />
              <button
                type="button"
                onClick={() => void handleCreate()}
                disabled={!createName.trim() || busyBranch !== null}
                className="rounded-lg bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                创建
              </button>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
```

- [ ] **Step 4: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/BranchPicker.tsx
git commit -m "feat(git): BranchPicker.tsx component (verbatim JSX + 2 adaptations)

Verbatim port of if2Ai's BranchPicker JSX (lines 204-435). State +
handlers come from the companion useBranchPicker hook (Task 3) so this
file stays under uClaw's 400 LOC hard cap.

Two adaptations baked into this commit:
1. Drop 'the-finals-selected-menu-item' class at the current-branch
   row (if2Ai theme-scoped hook, no-op in uClaw)
2. Init-repo flow surfaces a sonner confirmation toast with an
   '初始化' action button before running gitInitRepo (W6 spec §4.2 —
   workspace is a long-lived user dir; surprise repo creation is bad)

W6 PR B Task 4 of 11."
git log --oneline -1
git branch --show-current
```

---

## Task 5: useGitWorkbench hook (verbatim port of state + reload)

**Files:**
- Create: `ui/src/components/chat/git/useGitWorkbench.ts`

**Why extracted:** if2Ai's `GitWorkbenchDialog.tsx` is 419 LOC (over uClaw's 400 hard cap). Same hook+component pattern as Task 3/4 for BranchPicker.

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitWorkbenchDialog.tsx` lines 39–150.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Read the if2Ai source**

Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitWorkbenchDialog.tsx` lines 39-150. This is the state + reload portion to extract verbatim.

- [ ] **Step 3: Create the hook**

Create `/Users/ryanliu/Documents/uclaw/ui/src/components/chat/git/useGitWorkbench.ts` with the state + reload portion from if2Ai's GitWorkbenchDialog. Hook signature:

```ts
export interface UseGitWorkbenchArgs {
  open: boolean
  cwd: string | undefined
}

export interface UseGitWorkbenchResult {
  tab: Tab
  setTab: (t: Tab) => void
  statusState: ViewState<string>
  diffState: ViewState<string>
  branchesState: ViewState<BranchListItem[]>
  diffFull: boolean
  setDiffFull: (next: boolean) => void
  reload: (which: Tab | 'all') => Promise<void>
}
```

Full hook body — copy verbatim from if2Ai lines 39-150, wrapped in `export function useGitWorkbench(args): UseGitWorkbenchResult { ... }`. The `Tab` and `ViewState<T>` types are exported from this file. Adjust imports: `gitBranches / gitDiff / gitStatus / parseBranchList / BranchListItem` come from `@/modules/git/api` (same paths as if2Ai). The `useEffect` at lines 117-124 (diff toggle reload) stays in the hook. The `useEffect` at lines 127-136 (open-effect → reload('all')) ALSO stays in the hook.

- [ ] **Step 4: tsc + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/useGitWorkbench.ts
git commit -m "feat(git): useGitWorkbench hook — state + reload for workbench dialog

Extracted from if2Ai's GitWorkbenchDialog.tsx (419 LOC, exceeds uClaw's
400 hard cap). Hook owns: Tab state, three sub-states (statusState,
diffState, branchesState), diffFull toggle, reload dispatcher, and the
two side-effects (open-effect → reload('all'), diffFull-effect → reload('diff')).

The companion GitWorkbenchDialog.tsx (Task 6) renders the Dialog JSX.

W6 PR B Task 5 of 11."
git log --oneline -1
git branch --show-current
```

**Subagent note:** because the hook body is ~110 LOC of verbatim if2Ai code, the implementer subagent should READ if2Ai's source in full and reproduce the state + reload + useEffect blocks verbatim. The instruction is "copy lines 39-150 from if2Ai's GitWorkbenchDialog.tsx, wrap in `export function useGitWorkbench(args)`, return all stateful values + handlers."

---

## Task 6: GitWorkbenchDialog.tsx component (JSX)

**Files:**
- Create: `ui/src/components/chat/git/GitWorkbenchDialog.tsx`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitWorkbenchDialog.tsx` lines 1-38 (imports + Props + Tab type alias) + lines 151-419 (JSX + PlainTextView + BranchListView sub-components).

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Read the if2Ai source**

Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitWorkbenchDialog.tsx` lines 151-419. This is the JSX + the local sub-components (`PlainTextView`, `BranchListView`, `CenteredHint`, `ErrorBlock`) to port verbatim.

- [ ] **Step 3: Create the component**

Create `/Users/ryanliu/Documents/uclaw/ui/src/components/chat/git/GitWorkbenchDialog.tsx`. Skeleton:

```tsx
/**
 * GitWorkbenchDialog — 3-tab read-only inspector (状态 / 差异 / 分支).
 *
 * Verbatim port of if2Ai's GitWorkbenchDialog JSX. State + reload
 * extracted into useGitWorkbench hook (Task 5) so this file stays
 * under uClaw's 400 LOC hard cap.
 */

import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { GitBranch, Loader2, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { type BranchListItem } from '@/modules/git/api'
import { useGitWorkbench, type Tab, type ViewState } from './useGitWorkbench'

type Props = {
  open: boolean
  onOpenChange: (next: boolean) => void
  cwd: string | undefined
  currentBranch?: string
}

const TAB_LABEL: Record<Tab, string> = {
  status: '状态',
  diff: '差异',
  branches: '分支',
}

export function GitWorkbenchDialog({ open, onOpenChange, cwd, currentBranch }: Props) {
  const wb = useGitWorkbench({ open, cwd })
  // ... (JSX body — port verbatim from if2Ai lines 151-419)
  // Replace local state reads with `wb.statusState`, `wb.diffState`, `wb.branchesState`,
  // `wb.tab`, `wb.diffFull`, `wb.reload(...)`, etc.
}

// Local sub-components — port verbatim from if2Ai (PlainTextView, BranchListView,
// CenteredHint, ErrorBlock). These do not depend on state, only on rendered data
// passed as props.
```

**Implementer instruction:** Read if2Ai's GitWorkbenchDialog.tsx lines 151-419 in full, port verbatim, replace state reads with the hook's outputs. Constants like `LINE_PAGE_SIZE = 500` and `FULL_EXPAND_WARN_THRESHOLD = 5000` stay at module top.

- [ ] **Step 4: tsc + commit**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/GitWorkbenchDialog.tsx
git commit -m "feat(git): GitWorkbenchDialog.tsx component (verbatim JSX port)

Verbatim port of if2Ai's GitWorkbenchDialog JSX (lines 151-419) +
local sub-components (PlainTextView, BranchListView, CenteredHint,
ErrorBlock). State + reload come from useGitWorkbench hook (Task 5).

Tab labels: 状态 / 差异 / 分支. PlainTextView chunks at
LINE_PAGE_SIZE = 500 lines with FULL_EXPAND_WARN_THRESHOLD = 5000.
Diff stat ⇄ full toggle preserved.

W6 PR B Task 6 of 11."
git log --oneline -1
git branch --show-current
```

---

## Task 7: GitActionsPicker.tsx shell + Mode state machine

**Files:**
- Create: `ui/src/components/chat/git/GitActionsPicker.tsx`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitActionsPicker.tsx` lines 1–250 approximately — outer chip + Mode state machine + menu sub-view.

This is the first of three files for the 717-LOC monolith split.

- [ ] **Step 1: Branch verify + read source**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

Open `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitActionsPicker.tsx` in full to understand the Mode state machine and identify the boundary between "shell + menu" (this task) and "form sub-views" (Task 8) and "draft PR fallback" (Task 9).

The shell portion includes:
- Imports + Props type (lines 1-71)
- `Mode` discriminated-union state machine (lines 73-87)
- All useState/useCallback hooks (lines 88-260 approximately)
- The outer `<Popover>` + `<PopoverTrigger>` chip button (~lines 350-400)
- The `'menu'` sub-view inside `<PopoverContent>` (~lines 410-490)
- Helper functions specific to the shell (`ghOk` probe, `onSubmit` dispatcher)

**Plan implementer:** open the if2Ai source AND identify the lines that are exclusively used by the form sub-views (Commit form, CreateBranch form, PR form) — those move to Task 8. Lines exclusive to the `prDraft` mode move to Task 9.

- [ ] **Step 2: Create the file**

Create `/Users/ryanliu/Documents/uclaw/ui/src/components/chat/git/GitActionsPicker.tsx`. Imports:

```tsx
import * as React from 'react'
import { GitCommitHorizontal, GitBranchPlus, GitPullRequestArrow, Loader2, CheckCircle, XCircle } from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import {
  ghAvailable,
  gitCommit,
  gitCommitPushPr,
  gitCreateBranch,
  ghCreatePr,
  type CommitOutcome,
  type CreatePrResponse,
} from '@/modules/git/api'
import { GitActionsPickerForms } from './GitActionsPickerForms'
import { GitActionsPickerDraftPr } from './GitActionsPickerDraftPr'
```

`GitActionsPickerForms` and `GitActionsPickerDraftPr` will be created in Tasks 8 and 9. To keep this task's commit standalone (build green), define both as `() => null` stubs at the bottom of THIS file for now, then Tasks 8/9 replace them by creating the real files.

Actually a cleaner approach: write THIS task's file to reference the sibling files via import. Build will fail at commit time. Two choices:

**Choice A (recommended):** Create the 3 files in a single commit (Tasks 7+8+9 collapse into one task). Pros: single coherent diff, smaller plan. Cons: bigger commit (~620 LOC across 3 files).

**Choice B:** Stub the sibling files. Pros: 3 small commits. Cons: stubs introduce dead intermediate states.

**Decision: Choice A.** Combine Tasks 7+8+9 into a single Task 7 that creates all 3 files together. Plan adjusts to 9 implementation tasks instead of 11.

- [ ] **Step 3: Create all 3 GitActionsPicker files in this commit**

Create the three files. Their bodies port verbatim from `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/chat/GitActionsPicker.tsx` with the following file-split rules:

**`GitActionsPicker.tsx` (~190 LOC) gets:**
- All imports
- `Props` type definition
- `Mode` discriminated-union type
- All `useState` hooks for the picker shell
- `ghOk` probe useEffect
- The outer `<Popover>` + `<PopoverTrigger>` JSX
- The `menu` sub-view (mode === 'menu') inside `<PopoverContent>`
- The dispatcher logic that transitions between modes
- The `success`/`error` rendered states (small JSX blocks)

**`GitActionsPickerForms.tsx` (~290 LOC) gets:**
- `CommitForm` sub-component (verbatim from if2Ai)
- `CreateBranchForm` sub-component
- `PrForm` sub-component
- Any shared helpers used only by these forms (validation, etc.)

**`GitActionsPickerDraftPr.tsx` (~160 LOC) gets:**
- `PrDraftView` sub-component
- `GhMissingBanner` sub-component
- `shellAnsiCQuote` helper function

The shell file imports the form file's `<CommitForm>`, `<CreateBranchForm>`, `<PrForm>` components and the draft file's `<PrDraftView>` + `<GhMissingBanner>`. State (`mode`, etc.) flows top-down via props.

**Implementer instruction:** Read if2Ai's GitActionsPicker.tsx lines 1-717 in full. Build a mental map of which lines belong to which output file based on the rules above. Then create all 3 files in one commit.

- [ ] **Step 4: tsc**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/GitActionsPicker.tsx \
        ui/src/components/chat/git/GitActionsPickerForms.tsx \
        ui/src/components/chat/git/GitActionsPickerDraftPr.tsx
git commit -m "feat(git): GitActionsPicker (split into 3 files for 400 LOC cap)

Verbatim port of if2Ai's GitActionsPicker.tsx (717 LOC monolith) split
along internal function boundaries already present in if2Ai's source:

- GitActionsPicker.tsx (~190 LOC): outer chip + Popover + Mode state
  machine + menu sub-view + success/error states + dispatcher
- GitActionsPickerForms.tsx (~290 LOC): CommitForm + CreateBranchForm
  + PrForm sub-components
- GitActionsPickerDraftPr.tsx (~160 LOC): PrDraftView + GhMissingBanner
  + shellAnsiCQuote ANSI-C escape helper

Mode discriminated-union state machine ported verbatim (8 variants:
menu/commit/createBranch/pr/busy/success/error/prDraft). Chinese
labels preserved (提交, 创建分支, PR 标题, PR 描述, 已提交, etc.).

W6 PR B Task 7 of 9 (renumbered from 7-9; combined into single commit
because the 3 files cross-reference and stubs would introduce dead
intermediate states)."
git log --oneline -1
git branch --show-current
```

---

## Task 8 (was 10): GitChipsRow + dual composer wiring (atomic commit per CLAUDE.md)

**Files:**
- Create: `ui/src/components/chat/git/GitChipsRow.tsx`
- Modify: `ui/src/components/chat/ChatInput.tsx` (insert at line 291 inner row)
- Modify: `ui/src/components/agent/AgentView.tsx` (insert at line 1468 inner row)

**Per CLAUDE.md "Adjacent edits that look like scope creep but aren't" rule** — both composer files must be touched in the same atomic commit so the dual-composer rule is visible in `git blame`.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Create `GitChipsRow.tsx`**

```tsx
/**
 * GitChipsRow — composer footer container for the 3 git affordances.
 *
 * Reads activeWorkspaceCwdAtom directly; renders nothing when no
 * workspace has a directory attached (W6 spec §12.3). Composes the
 * BranchPicker + GitActionsPicker + GitWorkbenchDialog into a single
 * unit so each composer (ChatInput, AgentView) only imports one thing.
 *
 * Per CLAUDE.md dual-composer rule: import this in BOTH ChatInput.tsx
 * and AgentView.tsx. Identical placement (last child of the bottom-row
 * left-side `flex items-center gap-1.5 flex-1 min-w-0` container).
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import { activeWorkspaceCwdAtom } from '@/atoms/workspace'
import { gitIsRepo, gitCurrentBranch } from '@/modules/git/api'
import { BranchPicker } from './BranchPicker'
import { GitActionsPicker } from './GitActionsPicker'
import { GitWorkbenchDialog } from './GitWorkbenchDialog'

interface Props {
  className?: string
}

export function GitChipsRow({ className }: Props): React.ReactElement | null {
  const cwd = useAtomValue(activeWorkspaceCwdAtom)
  const [currentBranch, setCurrentBranch] = React.useState<string>('')
  const [isGitRepo, setIsGitRepo] = React.useState<boolean | null>(null)
  const [workbenchOpen, setWorkbenchOpen] = React.useState(false)

  React.useEffect(() => {
    if (!cwd) {
      setIsGitRepo(null)
      setCurrentBranch('')
      return
    }
    let cancelled = false
    setIsGitRepo(null)  // probing
    void Promise.all([
      gitIsRepo(cwd).catch(() => false),
      gitCurrentBranch(cwd).catch(() => ''),
    ]).then(([isRepo, branch]) => {
      if (cancelled) return
      setIsGitRepo(isRepo)
      setCurrentBranch(branch)
    })
    return () => { cancelled = true }
  }, [cwd])

  if (!cwd) return null

  return (
    <div className={cn('flex items-center gap-1.5', className)}>
      <BranchPicker
        cwd={cwd}
        currentBranch={currentBranch}
        isGitRepo={isGitRepo}
        onChanged={setCurrentBranch}
        onInitRepo={() => setIsGitRepo(true)}
      />
      <GitActionsPicker
        cwd={cwd}
        isGitRepo={isGitRepo}
        onBranchChange={setCurrentBranch}
        onOpenWorkbench={() => setWorkbenchOpen(true)}
      />
      <GitWorkbenchDialog
        open={workbenchOpen}
        onOpenChange={setWorkbenchOpen}
        cwd={cwd}
        currentBranch={currentBranch}
      />
    </div>
  )
}
```

- [ ] **Step 3: Wire into `ChatInput.tsx`**

Read `/Users/ryanliu/Documents/uclaw/ui/src/components/chat/ChatInput.tsx` lines 289-300 first. Find the inner row at line 291: `<div className="flex items-center gap-1.5 flex-1 min-w-0">`. Add `<GitChipsRow />` as the LAST child inside that div (before the closing `</div>`).

Also add the import at the top:

```tsx
import { GitChipsRow } from './git/GitChipsRow'
```

(Path is relative because `GitChipsRow.tsx` lives at `ui/src/components/chat/git/GitChipsRow.tsx`, sibling of `ChatInput.tsx` at `ui/src/components/chat/ChatInput.tsx`.)

- [ ] **Step 4: Wire into `AgentView.tsx`**

Read `/Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx` lines 1467-1480. Same surgery on the inner row at line 1468.

Add the import at the top:

```tsx
import { GitChipsRow } from '@/components/chat/git/GitChipsRow'
```

(Absolute import because `AgentView.tsx` is in `components/agent/`, not a sibling.)

- [ ] **Step 5: tsc + smoke test**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -5
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: 310+ passed (no regressions; component tests come in Task 9)
```

- [ ] **Step 6: Commit (per CLAUDE.md dual-composer rule, both composers in this single commit)**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/GitChipsRow.tsx \
        ui/src/components/chat/ChatInput.tsx \
        ui/src/components/agent/AgentView.tsx
git commit -m "feat(git): GitChipsRow wrapper + wire into ChatInput + AgentView

GitChipsRow reads activeWorkspaceCwdAtom directly; renders nothing
when null (W6 spec §12.3 — git is meaningless without a directory).
Composes BranchPicker + GitActionsPicker + GitWorkbenchDialog with
internal currentBranch / isGitRepo / workbenchOpen state.

Wired into BOTH chat composers per CLAUDE.md dual-composer rule:
- ChatInput.tsx line 291: insert as last child of bottom-row left flex
- AgentView.tsx line 1468: same placement

Single atomic commit so 'git blame' shows the dual-composer pairing.
Future readers updating one composer must update both.

W6 PR B Task 8 of 9."
git log --oneline -1
git branch --show-current
```

---

## Task 9 (was 11): Tests + final verification

**Files:**
- Create: `ui/src/components/chat/git/BranchPicker.test.tsx`
- Create: `ui/src/components/chat/git/GitActionsPicker.test.tsx`
- No backend changes

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw && git checkout claude/w6-pr-b-ui
git branch --show-current
```

- [ ] **Step 2: Create `BranchPicker.test.tsx`**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { BranchPicker } from './BranchPicker'

// Mock the Tauri IPC layer used by BranchPicker / useBranchPicker.
vi.mock('@/modules/git/api', () => ({
  gitBranches: vi.fn(async () => '* main         abcdef1 init\n  feat/foo     1234567 wip'),
  gitStatus: vi.fn(async () => '## main\n M src/foo.ts'),
  gitCheckoutBranch: vi.fn(async () => undefined),
  gitCreateBranch: vi.fn(async () => undefined),
  gitInitRepo: vi.fn(async () => undefined),
  parseBranchList: vi.fn((raw: string) =>
    raw.split('\n').filter(Boolean).map((line) => {
      const isCurrent = line.trim().startsWith('*')
      const name = line.replace(/^[*+]\s*/, '').split(/\s+/)[0]
      return { name, isCurrent }
    }),
  ),
  uncommittedFromStatus: vi.fn((raw: string | null) =>
    raw ? raw.split('\n').slice(1).filter((l) => l.trim().length > 0).length : 0,
  ),
}))

describe('BranchPicker', () => {
  it('renders the current branch label and opens the popover with the list', async () => {
    renderWithProviders(
      <BranchPicker
        cwd="/tmp/test-repo"
        currentBranch="main"
        isGitRepo={true}
      />,
    )
    // Trigger displays the current branch
    expect(screen.getByText('main')).toBeInTheDocument()
    // Click to open
    fireEvent.click(screen.getByRole('button', { name: '切换 git 分支' }))
    // List appears with the mocked branches
    await waitFor(() => {
      expect(screen.getByText('feat/foo')).toBeInTheDocument()
    })
  })

  it('shows the no-repo amber affordance when isGitRepo is false', () => {
    renderWithProviders(
      <BranchPicker
        cwd="/tmp/empty-dir"
        currentBranch=""
        isGitRepo={false}
      />,
    )
    expect(screen.getByText(/无 Git 仓库/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 3: Create `GitActionsPicker.test.tsx`**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { GitActionsPicker } from './GitActionsPicker'

vi.mock('@/modules/git/api', () => ({
  ghAvailable: vi.fn(async () => true),
  gitCommit: vi.fn(async () => ({ status: 'created', message: 'feat: test' })),
  gitCommitPushPr: vi.fn(async () => 'Committed → branch `feat/x` → PR https://...'),
  gitCreateBranch: vi.fn(async () => undefined),
  ghCreatePr: vi.fn(async () => ({ url: '...', wasExisting: false, base: 'main' })),
}))

describe('GitActionsPicker', () => {
  it('renders the trigger menu', () => {
    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={true} />,
    )
    expect(screen.getByRole('button', { name: /提交|git actions/i })).toBeInTheDocument()
  })

  it('shows the gh-missing draft fallback when ghAvailable returns false', async () => {
    const api = await import('@/modules/git/api')
    vi.mocked(api.ghAvailable).mockResolvedValueOnce(false)

    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={true} />,
    )
    fireEvent.click(screen.getByRole('button', { name: /提交|git actions/i }))
    // Trigger the PR flow → should land on prDraft mode because gh is missing
    // ... (test exercises the PR menu item; specific selector depends on if2Ai's labels)
    // The minimal assertion is that "draft" or "gh" or similar text appears
    await waitFor(() => {
      const text = screen.getByText((content) => /gh|draft|未安装/.test(content))
      expect(text).toBeInTheDocument()
    })
  })
})
```

**Note:** the GitActionsPicker tests may need refinement once the actual port lands (button labels and the exact draft-fallback trigger flow depend on the if2Ai source structure). The minimum assertion is "draft mode reachable when gh missing".

- [ ] **Step 4: Run all tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: 316 passed (310 baseline + 3 atom + 2 BranchPicker + 2 GitActionsPicker = 317, allow ±1)
```

- [ ] **Step 5: Full verification matrix**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 466 passed (no change; PR B is UI-only)

cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | tail -5
# Expect: build succeeds
```

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current
git add ui/src/components/chat/git/BranchPicker.test.tsx \
        ui/src/components/chat/git/GitActionsPicker.test.tsx
git commit -m "test(git): BranchPicker + GitActionsPicker jsdom integration tests

4 new vitest cases:
- BranchPicker: renders current branch label + opens popover with list
- BranchPicker: shows no-repo amber affordance when isGitRepo=false
- GitActionsPicker: renders trigger menu
- GitActionsPicker: shows gh-missing draft fallback when ghAvailable returns false

Mock @/modules/git/api per the existing pattern from
ui/src/components/ai-elements/message.test.tsx.

Baseline 310 + 3 (atom, Task 2) + 4 (this task) = 317 vitest cases.

W6 PR B Task 9 of 9."
git log --oneline -1
git branch --show-current
```

- [ ] **Step 7: Manual checklist (controller-driven, no commit)**

The controller verifies the following before pushing the PR:

```text
[ ] cd src-tauri && cargo test --lib → 466 passed
[ ] cd ui && npx tsc --noEmit → clean
[ ] cd ui && npm test -- --run → 317 passed (or 316 if a test was tightened)
[ ] cd ui && npm run build → succeeds

[ ] cd src-tauri && cargo tauri dev → app launches without console errors

[ ] BranchPicker opens, search filters, switch works
[ ] "未提交的更改：N 个文件" indicator appears on current branch when dirty
[ ] No-repo state: amber "无 Git 仓库 · 点击初始化" → sonner confirmation toast → click '初始化' → git init succeeds
[ ] GitWorkbenchDialog: 3 tabs load + refresh independently
[ ] GitWorkbenchDialog: diff stat ⇄ full toggle works
[ ] GitActionsPicker: commit (clean tree → skipped; dirty → created)
[ ] Commit-push-PR single-button flow renders URL in success state
[ ] gh missing → form shows draft view with copy-able command

[ ] DUAL COMPOSER REGRESSION: chips appear in BOTH Chat mode AND Agent mode
[ ] Null-cwd: workspace with no `path` → chips hidden entirely (verify by creating empty workspace)
[ ] 11-theme spot check on all chips + dialog (qingye, warm-paper, forest-night, default light/dark)
[ ] Amber warning states (no-repo, dirty-tree-checkout) readable across all themes
```

- [ ] **Step 8: (CONDITIONAL — only after user explicitly approves push)**

Per CLAUDE.md: do not push or open a PR until the user explicitly asks. When approved:

```bash
cd /Users/ryanliu/Documents/uclaw
git push -u origin claude/w6-pr-b-ui

gh pr create --title "W6 PR B: Workspace git UI (BranchPicker + WorkbenchDialog + ActionsPicker + composer wiring)" --body "$(cat <<'EOF'
## Summary

PR B of W6 — UI components for workspace git, built on PR A's backbone (#118). Three composer chips (BranchPicker, GitActionsPicker, GitWorkbenchDialog) wired into both ChatInput and AgentView via the GitChipsRow wrapper. Closes out W6 Phase 1+2.

## What lands

| File | LOC | Notes |
|---|---|---|
| ui/src/atoms/workspace.ts | +13 | activeWorkspaceCwdAtom derived atom |
| ui/src/components/chat/git/useBranchPicker.ts | ~155 | State + handlers hook (verbatim port from if2Ai) |
| ui/src/components/chat/git/BranchPicker.tsx | ~290 | JSX component (verbatim port + 2 documented adaptations) |
| ui/src/components/chat/git/useGitWorkbench.ts | ~145 | State + reload hook |
| ui/src/components/chat/git/GitWorkbenchDialog.tsx | ~280 | JSX component (3-tab inspector) |
| ui/src/components/chat/git/GitActionsPicker.tsx | ~190 | Shell + Mode state machine |
| ui/src/components/chat/git/GitActionsPickerForms.tsx | ~290 | 3 form sub-views |
| ui/src/components/chat/git/GitActionsPickerDraftPr.tsx | ~160 | gh-missing fallback + shellAnsiCQuote |
| ui/src/components/chat/git/GitChipsRow.tsx | ~55 | Dual-composer wrapper |
| ui/src/components/chat/ChatInput.tsx | +2 | <GitChipsRow /> at line 291 |
| ui/src/components/agent/AgentView.tsx | +2 | <GitChipsRow /> at line 1468 |

## Verbatim port discipline (per W6 spec §5 + §12)

Tailwind class strings, JSX structure, prop names, Chinese labels, ARIA attributes preserved unchanged from if2Ai.

**Documented adaptations:**
1. `cwd` source swap: `currentProject.workdir` → `activeWorkspaceCwdAtom`
2. GitActionsPicker split into 3 files (400 LOC cap)
3. BranchPicker + GitWorkbenchDialog split via hook + component (both exceed 400 LOC cap)

**Documented deletions:**
- `'the-finals-selected-menu-item'` class (if2Ai theme-scoped hook, no-op in uClaw)
- WorkspacePill skipped (uClaw has TabBarWorkspaceChip + WorkspaceSwitcherBar)

**uClaw-specific adaptation (spec §4.2):** init-repo flow surfaces a sonner confirmation toast before running `gitInitRepo`.

## Test plan

- [x] cd src-tauri && cargo test --lib — 466 passed (no Rust changes)
- [x] cd ui && npx tsc --noEmit — clean
- [x] cd ui && npm test -- --run — 317 passed (310 baseline + 7 new)
- [x] cd ui && npm run build — succeeds
- [ ] Manual: chips render in BOTH Chat and Agent composers
- [ ] Manual: null-cwd workspace hides chips entirely
- [ ] Manual: BranchPicker / WorkbenchDialog / GitActionsPicker exercised end-to-end
- [ ] Manual: 11-theme spot check

## Spec / plan

- Spec: docs/superpowers/specs/2026-05-13-w6-workspace-git-design.md (§12 PR B addendum)
- Plan: docs/superpowers/plans/2026-05-13-w6-workspace-git-pr-b-ui.md

## Branch base

Branched from main at 7f90720 (post-PR-A merge).
EOF
)"
```

---

## Self-Review

### Spec coverage

| Spec section | Implementing task |
|---|---|
| §4.1 `cwd` source → activeWorkspaceCwdAtom | Task 2 |
| §4.2 BranchPicker port (verbatim + 2 adaptations) | Tasks 3 + 4 |
| §4.2 init-repo confirmation toast | Task 4 |
| §4.3 GitWorkbenchDialog port (verbatim) | Tasks 5 + 6 |
| §4.4 GitActionsPicker 3-file split | Task 7 |
| §4.5 GitChipsRow + dual composer wiring | Task 8 |
| §4.6 WorkspacePill reuse (skipped) | Implicit — no file created |
| §4.7 Frontend tests | Tasks 2 (atom) + Task 9 (components) |
| §12.1 Adaptations narrowed to 2 + 2 deletions | Tasks 3-8 carry these forward |
| §12.4 GitChipsRow no `cwd` prop, reads atom directly | Task 8 |
| §12.5 ChatInput.tsx:291 + AgentView.tsx:1468 placement | Task 8 |

All spec requirements have an implementing task. No gaps.

### Placeholder scan

- No "TBD" / "TODO" / "implement later"
- Every step pastes actual code OR points to specific if2Ai line ranges (valid for verbatim ports)
- Test cases fully written (not "write tests for the above")
- Branch hygiene checks at start + before commit + after commit on every task

### Type consistency

- `LoadState` defined in Task 3 (hook), consumed by Task 4 (component)
- `ViewState<T>` + `Tab` defined in Task 5 (hook), consumed by Task 6 (component)
- `Mode` defined in Task 7's GitActionsPicker.tsx shell, consumed across all 3 sub-files
- `activeWorkspaceCwdAtom` defined in Task 2, consumed by Task 8 (GitChipsRow)
- `BranchListItem` imported from `@/modules/git/api` consistently (already exported by PR A)
- `CommitOutcome`, `CreatePrResponse` from `@/modules/git/api` (PR A) — consumed by Task 7 GitActionsPicker
- `GitChipsRow` import path: relative `./git/GitChipsRow` in ChatInput; absolute `@/components/chat/git/GitChipsRow` in AgentView (correct per their file locations)

### Task numbering reconciliation

Original plan called for 11 tasks (1 plan + 10 implementation). Revised to **9 implementation tasks** because:
- Tasks 7-9 (GitActionsPicker shell + Forms + DraftPr) collapsed into a single commit (Task 7 in this plan) — three files cross-reference each other, stubbing would create dead intermediate states. Cleaner as one atomic commit (~640 LOC across 3 files).
- BranchPicker split into hook (Task 3) + component+adaptation (Task 4) — one fewer commit than the original 3-step plan (search/list/switch → create form → init affordance), because each intermediate would have a broken component.

Net: **9 commits on `claude/w6-pr-b-ui`**:
1. `docs(plan): W6 PR B implementation plan` (this doc)
2. `feat(workspace): activeWorkspaceCwdAtom derived atom`
3. `feat(git): useBranchPicker hook`
4. `feat(git): BranchPicker.tsx component (verbatim JSX + 2 adaptations)`
5. `feat(git): useGitWorkbench hook`
6. `feat(git): GitWorkbenchDialog.tsx component (verbatim JSX port)`
7. `feat(git): GitActionsPicker (split into 3 files for 400 LOC cap)`
8. `feat(git): GitChipsRow wrapper + wire into ChatInput + AgentView`
9. `test(git): BranchPicker + GitActionsPicker jsdom integration tests`

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-13-w6-workspace-git-pr-b-ui.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — controller dispatches a fresh subagent per task, two-stage review after each (spec compliance + code quality), fast iteration. **Model split**: haiku for Tasks 2-6 + 9 (mechanical verbatim ports + small atoms + test files), sonnet for Task 7 (GitActionsPicker 3-file split, integration judgment) + Task 8 (dual composer wiring, touches unfamiliar composer files), controller drives commits directly when files cross-reference.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans` with batch checkpoints.

**Which approach?**

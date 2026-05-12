# Sidebar Git-Actions Relocation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the `提交 ▾` git-actions chip from both chat composers into the agent-mode left sidebar, beside the existing MCP·Skills capability row, separated by a hairline divider — without touching `BranchPicker`'s position in the composer.

**Architecture:** Reduce `GitChipsRow` to just `BranchPicker`. Add `variant: 'composer' | 'sidebar'` to `GitActionsPicker` so the same component renders two trigger styles. Create a new `SidebarGitActions` wrapper that owns its own cwd → `gitIsRepo` + `gitCurrentBranch` probe, the workbench dialog, and the hairline divider. Introduce a single `branchSyncTickAtom` so the composer's `BranchPicker` re-probes the current branch after the sidebar's "create branch" flow.

**Tech Stack:** React 18, TypeScript, Jotai, Radix Popover/Tooltip, Tailwind, Vitest + React Testing Library (jsdom). All changes are in `ui/`. No Rust, no migration, no CSP changes.

**Spec:** [docs/superpowers/specs/2026-05-13-sidebar-git-actions-relocation-design.md](../specs/2026-05-13-sidebar-git-actions-relocation-design.md)

---

## File Structure

**Create:**
- `ui/src/components/app-shell/SidebarGitActions.tsx` — sidebar wrapper that probes cwd, renders the hairline divider + `GitActionsPicker variant="sidebar"` + `GitWorkbenchDialog`.
- `ui/src/components/app-shell/SidebarGitActions.test.tsx` — render + behavior tests.

**Modify:**
- `ui/src/atoms/workspace.ts` — add `branchSyncTickAtom` (counter) for cross-surface branch refresh.
- `ui/src/components/chat/git/GitActionsPicker.tsx` — add `variant` prop; branch trigger className + popover side/align/sideOffset on it.
- `ui/src/components/chat/git/GitChipsRow.tsx` — drop `GitActionsPicker` + `GitWorkbenchDialog` rendering; include `branchSyncTickAtom` in the probe effect deps; update file docstring.
- `ui/src/components/app-shell/LeftSidebar.tsx` — wrap the MCP·Skills block + `<SidebarGitActions />` in a single two-section row; reduce the MCP·Skills button to `flex-1 min-w-0`.
- `ui/src/components/chat/git/GitActionsPicker.test.tsx` — add a `variant="sidebar"` assertion.

**Out of scope:**
- `BranchPicker`, `useBranchPicker`, `GitWorkbenchDialog` (internal contents), `useGitWorkbench`, `GitActionsPickerForms`, `GitActionsPickerDraftPr` — untouched.

---

## Task 1: Add `branchSyncTickAtom`

**Files:**
- Modify: `ui/src/atoms/workspace.ts` (end of file, after `syncWorkspaceSessionsAtom`)

- [ ] **Step 1: Add the atom**

Add at the end of `ui/src/atoms/workspace.ts`:

```ts
/**
 * Monotonic counter bumped whenever a git operation outside the
 * composer (today: the sidebar's GitActionsPicker create-branch flow)
 * needs the composer's BranchPicker to re-probe `gitCurrentBranch`.
 *
 * Consumers:
 * - GitChipsRow — includes the tick in its probe `useEffect` deps so a
 *   bump triggers re-probe of `gitCurrentBranch(cwd)`.
 * - SidebarGitActions — bumps the tick after `gitCreateBranch` succeeds.
 *
 * Idempotent: re-probing when nothing changed is a no-op (same value
 * comes back). Safe to bump from any caller after any branch mutation.
 */
export const branchSyncTickAtom = atom<number>(0)
```

- [ ] **Step 2: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors involving `workspace.ts`.

- [ ] **Step 3: Commit**

```bash
git add ui/src/atoms/workspace.ts
git commit -m "feat(atoms): add branchSyncTickAtom for cross-surface branch refresh"
```

---

## Task 2: Add `variant` prop to `GitActionsPicker`

**Files:**
- Modify: `ui/src/components/chat/git/GitActionsPicker.tsx` (Props type around L47-64; component signature at L85; trigger className at L275-300; PopoverContent at L301-305)
- Modify: `ui/src/components/chat/git/GitActionsPicker.test.tsx`

- [ ] **Step 1: Write the failing test**

Append to `ui/src/components/chat/git/GitActionsPicker.test.tsx`, inside the `describe('GitActionsPicker', …)` block:

```tsx
  it('applies sidebar variant trigger styling', () => {
    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={true} variant="sidebar" />,
    )
    const trigger = screen.getByRole('button', { name: 'Git 操作' })
    // Sidebar variant uses the muted text+rounded-[10px] style, not the
    // bordered px-3 py-1.5 composer chip.
    expect(trigger.className).toContain('rounded-[10px]')
    expect(trigger.className).toContain('text-foreground/50')
    expect(trigger.className).not.toContain('border')
    expect(trigger.className).not.toContain('rounded-lg')
  })

  it('defaults to composer variant when no variant prop is given', () => {
    renderWithProviders(
      <GitActionsPicker cwd="/tmp/test-repo" isGitRepo={true} />,
    )
    const trigger = screen.getByRole('button', { name: 'Git 操作' })
    expect(trigger.className).toContain('rounded-lg')
    expect(trigger.className).toContain('border')
  })
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run src/components/chat/git/GitActionsPicker.test.tsx 2>&1 | tail -20`
Expected: FAIL — `Type '{ … variant: "sidebar" }' is not assignable …` (compile-time) OR `expect(trigger.className).toContain('rounded-[10px]')` mismatch.

- [ ] **Step 3: Extend the `Props` type**

Edit `ui/src/components/chat/git/GitActionsPicker.tsx` — change the `Props` type (currently around L47-64). Add `variant` at the end:

```ts
type Props = {
  cwd: string | undefined
  isGitRepo?: boolean | null
  onGitRepoChanged?: () => void
  onBranchChange?: (newBranch: string) => void
  onOpenWorkbench?: () => void
  className?: string
  /**
   * Trigger style. `'composer'` (default) renders the bordered chip used
   * inside ChatInput / AgentView. `'sidebar'` renders a borderless row
   * matching the LeftSidebar's MCP·Skills aesthetic, with the popover
   * flipped to `side="right"` to expand into the chat-area whitespace.
   *
   * The popover BODY (menu, commit form, PR form, busy/success/error
   * views) is identical across variants.
   */
  variant?: 'composer' | 'sidebar'
}
```

- [ ] **Step 4: Destructure `variant` in the component**

Update the component signature at L85 — add `variant = 'composer'`:

```ts
export function GitActionsPicker({
  cwd,
  isGitRepo = null,
  onGitRepoChanged,
  onBranchChange,
  onOpenWorkbench,
  className,
  variant = 'composer',
}: Props) {
```

- [ ] **Step 5: Branch the trigger className**

Replace the trigger button block (currently L277-299) — wrap the className branch:

```tsx
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          className={cn(
            'window-no-drag inline-flex items-center transition-colors disabled:cursor-not-allowed disabled:opacity-60',
            variant === 'sidebar'
              ? cn(
                  'gap-1.5 rounded-[10px] px-3 py-2 text-[12px]',
                  noRepo
                    ? 'text-amber-600 hover:bg-amber-500/12 hover:text-amber-500'
                    : 'text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70',
                )
              : cn(
                  'gap-1.5 rounded-lg border px-3 py-1.5 text-[12px] font-medium',
                  noRepo
                    ? 'border-amber-200 bg-amber-50 text-amber-800 hover:border-amber-300 hover:bg-amber-100'
                    : 'border-border/70 text-muted-foreground hover:border-border hover:bg-accent hover:text-accent-foreground',
                ),
            className,
          )}
          data-window-no-drag="true"
          aria-label="Git 操作"
          title={noRepo ? '当前目录尚未初始化 Git — 点击查看初始化选项' : undefined}
        >
          {noRepo ? (
            <Sparkles
              className={variant === 'sidebar' ? 'h-[13px] w-[13px]' : 'h-3.5 w-3.5'}
              strokeWidth={1.75}
            />
          ) : (
            <GitCommitHorizontal
              className={variant === 'sidebar' ? 'h-[13px] w-[13px]' : 'h-3.5 w-3.5'}
              strokeWidth={1.75}
            />
          )}
          <span>{noRepo ? '初始化 Git' : '提交'}</span>
          <ChevronDown
            className={cn(
              variant === 'sidebar' ? 'h-[11px] w-[11px]' : 'h-3 w-3',
              variant === 'sidebar'
                ? (noRepo ? 'text-amber-600' : 'text-foreground/30')
                : (noRepo ? 'text-amber-600' : 'text-muted-foreground'),
            )}
          />
        </button>
      </PopoverTrigger>
```

- [ ] **Step 6: Branch the PopoverContent side / align / sideOffset**

Replace the `<PopoverContent>` opening tag (currently L301-305) — keep the composer behavior byte-identical (don't set `side`, let Radix default to `bottom` + collision-flip), only set `side`/override `align`/`sideOffset` for the sidebar variant. Find:

```tsx
      <PopoverContent
        align="center"
        sideOffset={12}
        collisionPadding={16}
```

Replace with:

```tsx
      <PopoverContent
        {...(variant === 'sidebar'
          ? { side: 'right' as const, align: 'start' as const, sideOffset: 8 }
          : { align: 'center' as const, sideOffset: 12 })}
        collisionPadding={16}
```

> Composer keeps `side` unset (Radix default `bottom` → collision-flips
> upward because the composer sits at page bottom). Sidebar pins
> `side="right"` so the popover expands into chat-area whitespace
> rather than overlapping sibling sidebar items.

- [ ] **Step 7: Run the test to verify it passes**

Run: `cd ui && npm test -- --run src/components/chat/git/GitActionsPicker.test.tsx 2>&1 | tail -20`
Expected: PASS — all 4 tests green (`renders the trigger button`, `renders with isGitRepo=null …`, `applies sidebar variant trigger styling`, `defaults to composer variant …`).

- [ ] **Step 8: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add ui/src/components/chat/git/GitActionsPicker.tsx ui/src/components/chat/git/GitActionsPicker.test.tsx
git commit -m "feat(git-actions): variant prop for composer vs sidebar trigger styling"
```

---

## Task 3: Create `SidebarGitActions` wrapper

**Files:**
- Create: `ui/src/components/app-shell/SidebarGitActions.tsx`
- Create: `ui/src/components/app-shell/SidebarGitActions.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/app-shell/SidebarGitActions.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { createStore } from 'jotai'
import { screen, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { workspacesAtom, activeWorkspaceIdAtom } from '@/atoms/workspace'
import { SidebarGitActions } from './SidebarGitActions'

vi.mock('@/modules/git/api', () => ({
  ghAvailable: vi.fn(async () => true),
  gitCommit: vi.fn(async () => ({ status: 'created', message: 'feat: test' })),
  gitCommitPushPr: vi.fn(async () => 'ok'),
  gitCreateBranch: vi.fn(async () => undefined),
  gitInitRepo: vi.fn(async () => undefined),
  gitIsRepo: vi.fn(async () => true),
  gitCurrentBranch: vi.fn(async () => 'main'),
  ghCreatePr: vi.fn(async () => ({ url: '', wasExisting: false, base: 'main' })),
}))

vi.mock('sonner', () => ({ toast: vi.fn() }))

function makeStore(cwd: string | null) {
  const store = createStore()
  if (cwd) {
    store.set(workspacesAtom, [
      {
        id: 'ws1',
        name: 'ws1',
        icon: '📁',
        path: cwd,
        attachedDirs: [],
        sortOrder: 0,
        createdAt: '',
        updatedAt: '',
      },
    ])
    store.set(activeWorkspaceIdAtom, 'ws1')
  }
  return store
}

describe('SidebarGitActions', () => {
  it('renders null when no workspace cwd is set', () => {
    const store = makeStore(null)
    const { container } = renderWithProviders(<SidebarGitActions />, { store })
    expect(container.firstChild).toBeNull()
  })

  it('renders the divider + sidebar-variant trigger when cwd is a git repo', async () => {
    const store = makeStore('/tmp/test-repo')
    renderWithProviders(<SidebarGitActions />, { store })
    // Trigger appears after the async probe resolves.
    const trigger = await screen.findByRole('button', { name: 'Git 操作' })
    expect(trigger.className).toContain('rounded-[10px]')
    // The hairline divider is the immediately-preceding sibling.
    const divider = trigger.previousElementSibling as HTMLElement | null
    expect(divider).not.toBeNull()
    expect(divider!.getAttribute('aria-hidden')).toBe('true')
    expect(divider!.className).toContain('w-px')
  })

  it('shows the amber init-Git trigger when cwd is not a git repo', async () => {
    const { gitIsRepo } = await import('@/modules/git/api')
    ;(gitIsRepo as ReturnType<typeof vi.fn>).mockResolvedValueOnce(false)
    const store = makeStore('/tmp/empty-dir')
    renderWithProviders(<SidebarGitActions />, { store })
    await waitFor(() => {
      expect(screen.getByText('初始化 Git')).toBeInTheDocument()
    })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npm test -- --run src/components/app-shell/SidebarGitActions.test.tsx 2>&1 | tail -20`
Expected: FAIL — `Cannot find module './SidebarGitActions'`.

- [ ] **Step 3: Create the component**

Create `ui/src/components/app-shell/SidebarGitActions.tsx`:

```tsx
/**
 * SidebarGitActions — left-sidebar host for the "提交 ▾" popover.
 *
 * Owns the `cwd → gitIsRepo + gitCurrentBranch` probe (mirrors
 * GitChipsRow's pattern). Renders nothing when no workspace has a
 * directory attached — the hairline divider lives inside this
 * component so the divider disappears along with the trigger,
 * leaving the MCP·Skills row to occupy the full width.
 *
 * The composer's BranchPicker re-probes its branch label via the
 * `branchSyncTickAtom` we bump after a successful create-branch flow.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { activeWorkspaceCwdAtom, branchSyncTickAtom } from '@/atoms/workspace'
import { gitIsRepo, gitCurrentBranch } from '@/modules/git/api'
import { GitActionsPicker } from '@/components/chat/git/GitActionsPicker'
import { GitWorkbenchDialog } from '@/components/chat/git/GitWorkbenchDialog'

export function SidebarGitActions(): React.ReactElement | null {
  const cwd = useAtomValue(activeWorkspaceCwdAtom)
  const bumpBranchTick = useSetAtom(branchSyncTickAtom)
  const [isRepo, setIsRepo] = React.useState<boolean | null>(null)
  const [currentBranch, setCurrentBranch] = React.useState<string>('')
  const [workbenchOpen, setWorkbenchOpen] = React.useState(false)

  React.useEffect(() => {
    if (!cwd) {
      setIsRepo(null)
      setCurrentBranch('')
      return
    }
    let cancelled = false
    setIsRepo(null)
    void Promise.all([
      gitIsRepo(cwd).catch(() => false),
      gitCurrentBranch(cwd).catch(() => ''),
    ]).then(([repo, branch]) => {
      if (cancelled) return
      setIsRepo(repo)
      setCurrentBranch(branch)
    })
    return () => { cancelled = true }
  }, [cwd])

  if (!cwd) return null

  return (
    <>
      <div
        aria-hidden="true"
        className="self-center w-px h-4 bg-foreground/10"
      />
      <GitActionsPicker
        variant="sidebar"
        cwd={cwd}
        isGitRepo={isRepo}
        onGitRepoChanged={() => setIsRepo(true)}
        onBranchChange={(name) => {
          // Keep local branch label fresh for the workbench dialog
          // header, and signal the composer's BranchPicker to re-probe.
          setCurrentBranch(name)
          bumpBranchTick((n) => n + 1)
        }}
        onOpenWorkbench={() => setWorkbenchOpen(true)}
      />
      <GitWorkbenchDialog
        open={workbenchOpen}
        onOpenChange={setWorkbenchOpen}
        cwd={cwd}
        currentBranch={currentBranch}
      />
    </>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ui && npm test -- --run src/components/app-shell/SidebarGitActions.test.tsx 2>&1 | tail -30`
Expected: PASS — all 3 cases (`renders null …`, `renders the divider + sidebar-variant trigger …`, `shows the amber init-Git trigger …`).

- [ ] **Step 5: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/app-shell/SidebarGitActions.tsx ui/src/components/app-shell/SidebarGitActions.test.tsx
git commit -m "feat(left-sidebar): SidebarGitActions wrapper (hidden divider + git-actions trigger)"
```

---

## Task 4: Reduce `GitChipsRow` to BranchPicker + tick-aware probe

**Files:**
- Modify: `ui/src/components/chat/git/GitChipsRow.tsx`

- [ ] **Step 1: Rewrite the file**

Replace the entire contents of `ui/src/components/chat/git/GitChipsRow.tsx` with:

```tsx
/**
 * GitChipsRow — composer footer container for the BranchPicker chip.
 *
 * Reads `activeWorkspaceCwdAtom` directly; renders nothing when no
 * workspace has a directory attached. Owns the `cwd → gitIsRepo +
 * gitCurrentBranch` probe and feeds the result into BranchPicker.
 *
 * History: this file used to also host GitActionsPicker + the
 * GitWorkbenchDialog. Both moved to `SidebarGitActions` (left
 * sidebar) on 2026-05-13. The composer keeps BranchPicker because
 * branch is a per-conversation routing decision, not a workspace-
 * level capability. See:
 *   docs/superpowers/specs/2026-05-13-sidebar-git-actions-relocation-design.md
 *
 * Cross-surface refresh: `branchSyncTickAtom` is included in the
 * probe `useEffect` deps. SidebarGitActions bumps the tick after a
 * successful create-branch flow, triggering this row to re-probe the
 * current branch.
 *
 * Per CLAUDE.md dual-composer rule: imported in BOTH ChatInput.tsx
 * and AgentView.tsx.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import { activeWorkspaceCwdAtom, branchSyncTickAtom } from '@/atoms/workspace'
import { gitIsRepo, gitCurrentBranch } from '@/modules/git/api'
import { BranchPicker } from './BranchPicker'

interface Props {
  className?: string
}

export function GitChipsRow({ className }: Props): React.ReactElement | null {
  const cwd = useAtomValue(activeWorkspaceCwdAtom)
  const branchSyncTick = useAtomValue(branchSyncTickAtom)
  const [currentBranch, setCurrentBranch] = React.useState<string>('')
  const [isGitRepo, setIsGitRepo] = React.useState<boolean | null>(null)

  React.useEffect(() => {
    if (!cwd) {
      setIsGitRepo(null)
      setCurrentBranch('')
      return
    }
    let cancelled = false
    setIsGitRepo(null)
    void Promise.all([
      gitIsRepo(cwd).catch(() => false),
      gitCurrentBranch(cwd).catch(() => ''),
    ]).then(([isRepo, branch]) => {
      if (cancelled) return
      setIsGitRepo(isRepo)
      setCurrentBranch(branch)
    })
    return () => { cancelled = true }
  }, [cwd, branchSyncTick])

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
    </div>
  )
}
```

- [ ] **Step 2: Run BranchPicker tests + type-check**

Run: `cd ui && npm test -- --run src/components/chat/git/BranchPicker.test.tsx 2>&1 | tail -10`
Expected: PASS — `BranchPicker` itself is unchanged, its tests stay green.

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors. (Removed imports — `GitActionsPicker`, `GitWorkbenchDialog` — were the only consumers in this file; no other tsx imports them via this path.)

- [ ] **Step 3: Run a wider test sweep to catch break-through**

Run: `cd ui && npm test -- --run 2>&1 | tail -20`
Expected: PASS. The full suite should be green; if any test imports `GitActionsPicker` / `GitWorkbenchDialog` through `GitChipsRow` indirectly, it will surface here.

- [ ] **Step 4: Commit**

```bash
git add ui/src/components/chat/git/GitChipsRow.tsx
git commit -m "refactor(git-chips): drop GitActionsPicker + workbench from composer footer"
```

---

## Task 5: Render `SidebarGitActions` inside LeftSidebar row

**Files:**
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx` (lines ~874-890, MCP·Skills block)

- [ ] **Step 1: Add the import**

Add to the existing `app-shell` imports block in `ui/src/components/app-shell/LeftSidebar.tsx` (next to `ModeSwitcher`):

```tsx
import { SidebarGitActions } from './SidebarGitActions'
```

- [ ] **Step 2: Replace the MCP·Skills block**

Find this existing block (around L874-890):

```tsx
      {/* Agent 模式：工作区能力指示器 */}
      {mode === 'agent' && capabilities && (
        <div className="px-3 pb-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <button onClick={() => { setSettingsTab('agent'); setSettingsOpen(true) }} className="w-full flex items-center gap-3 px-3 py-2 rounded-[10px] text-[12px] text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70 transition-colors titlebar-no-drag">
                <div className="flex items-center gap-2.5 flex-1 min-w-0">
                  <span className="flex items-center gap-1"><Plug size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.mcpServers.filter((s) => s.enabled).length}</span><span className="text-foreground/30">MCP</span></span>
                  <span className="text-foreground/20">·</span>
                  <span className="flex items-center gap-1"><Zap size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.skills.length}</span><span className="text-foreground/30">Skills</span></span>
                </div>
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">点击配置 MCP 与 Skills</TooltipContent>
          </Tooltip>
        </div>
      )}
```

Replace it with:

```tsx
      {/* Agent 模式：工作区能力指示器
       *
       * Two sections on one row, separated by a 1px hairline that
       * lives inside <SidebarGitActions /> (so it disappears along
       * with the git trigger when no workspace dir is attached):
       *   - MCP · Skills (always visible in agent mode w/ capabilities)
       *   - Git 提交 ▾   (only when activeWorkspaceCwd is set)
       *
       * Spec: docs/superpowers/specs/2026-05-13-sidebar-git-actions-relocation-design.md
       */}
      {mode === 'agent' && capabilities && (
        <div className="px-3 pb-1">
          <div className="flex items-stretch gap-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <button onClick={() => { setSettingsTab('agent'); setSettingsOpen(true) }} className="flex-1 min-w-0 flex items-center gap-3 px-3 py-2 rounded-[10px] text-[12px] text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70 transition-colors titlebar-no-drag">
                  <div className="flex items-center gap-2.5 flex-1 min-w-0">
                    <span className="flex items-center gap-1"><Plug size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.mcpServers.filter((s) => s.enabled).length}</span><span className="text-foreground/30">MCP</span></span>
                    <span className="text-foreground/20">·</span>
                    <span className="flex items-center gap-1"><Zap size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.skills.length}</span><span className="text-foreground/30">Skills</span></span>
                  </div>
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">点击配置 MCP 与 Skills</TooltipContent>
            </Tooltip>
            <SidebarGitActions />
          </div>
        </div>
      )}
```

Two changes vs the original: outer `<div>` now wraps the Tooltip + `<SidebarGitActions />` in a `flex items-stretch gap-2` container; the MCP·Skills button is `flex-1 min-w-0` instead of `w-full`.

- [ ] **Step 3: Type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: no errors.

- [ ] **Step 4: Run the full UI test suite**

Run: `cd ui && npm test -- --run 2>&1 | tail -20`
Expected: PASS — no regression.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/app-shell/LeftSidebar.tsx
git commit -m "feat(left-sidebar): host SidebarGitActions next to MCP·Skills row"
```

---

## Task 6: Remove the chat-composer 提交 button from both composers (verification)

**Goal:** Verify there is no remaining direct reference to `GitActionsPicker` outside the new sidebar wrapper. The earlier tasks already broke the composer's render path (Task 4 dropped `GitActionsPicker` from `GitChipsRow`), but ChatInput and AgentView still import `GitChipsRow` itself — that import stays unchanged.

**Files:**
- No edits (verification step only).

- [ ] **Step 1: Confirm no stale composer references**

Run:

```bash
grep -rn "GitActionsPicker\|GitWorkbenchDialog" ui/src --include="*.tsx" --include="*.ts" \
  | grep -v ".test." \
  | grep -v "GitActionsPicker.tsx" \
  | grep -v "GitActionsPickerForms" \
  | grep -v "GitActionsPickerDraftPr" \
  | grep -v "GitWorkbenchDialog.tsx" \
  | grep -v "SidebarGitActions.tsx" \
  | grep -v "useGitWorkbench"
```

Expected output: empty (or only doc-comment references like in `atoms/workspace.ts` and `modules/git/api.ts` — those are comments, not imports).

If any IMPORT line shows up: it is a stale import from before the relocation and must be removed in this step before continuing.

- [ ] **Step 2: Manual smoke (cannot be automated under jsdom)**

> **Skip if you cannot run `cargo tauri dev` — record this as deferred to the PR author and continue.**

Run: `cd src-tauri && cargo tauri dev`

After the window opens:
1. Switch to Agent mode in the LeftSidebar.
2. Confirm the row shows: `🔌 N MCP · ⚡ N Skills │ ⎇ 提交 ▾` (with a hairline between sections).
3. Click `提交 ▾` → popover expands to the right of the sidebar; menu visible.
4. Switch to Chat mode → confirm the composer footer no longer shows the 提交 chip but still shows `⎇ main`.
5. Repeat (3) for `AgentView` mode (the agent-mode composer): same — 提交 gone, `⎇ main` present.
6. Open a workspace with no attached directory (or an empty workspace) → confirm the sidebar row is just MCP·Skills with no orphan divider.
7. Open a workspace whose dir is not a git repo → sidebar shows amber `初始化 Git ▾` next to MCP·Skills.

- [ ] **Step 3: No commit**

This step is verification only; nothing to commit.

---

## Task 7: Final integration check

**Files:**
- No production code edits.

- [ ] **Step 1: Full type-check**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: clean.

- [ ] **Step 2: Full UI suite**

Run: `cd ui && npm test -- --run 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Confirm bisectable history**

Run: `git log --oneline main..HEAD`
Expected: 1 spec commit + 1 plan commit + 5 implementation commits = 7 commits, in this order:
1. `docs(spec): sidebar git-actions relocation design` (already on branch from the brainstorming step)
2. `docs(plan): sidebar git-actions relocation implementation plan` (commit the plan markdown before starting Task 1)
3. `feat(atoms): add branchSyncTickAtom for cross-surface branch refresh`
4. `feat(git-actions): variant prop for composer vs sidebar trigger styling`
5. `feat(left-sidebar): SidebarGitActions wrapper (hidden divider + git-actions trigger)`
6. `refactor(git-chips): drop GitActionsPicker + workbench from composer footer`
7. `feat(left-sidebar): host SidebarGitActions next to MCP·Skills row`

If commit count is off, rebase / squash before opening the PR.

- [ ] **Step 4: Open the PR**

Use `gh pr create` from the current branch (`claude/sidebar-git-actions`) targeting `main`. Body must include the `## Commits (bisectable)` table per CLAUDE.md project workflow.

---

## Self-Review Notes

- **Spec §2 (scope):** Tasks 2, 3, 5 implement the move. Task 4 reduces `GitChipsRow`. BranchPicker untouched ✓.
- **Spec §3.1 (component split):** Task 4 (GitChipsRow), Task 3 (SidebarGitActions).
- **Spec §3.2 (variant prop):** Task 2.
- **Spec §3.3 (LeftSidebar row):** Task 5.
- **Spec §3.4 (cross-surface sync):** Tasks 1, 3, 4. (No standalone test — the `onBranchChange → tick bump` is a one-line synchronous handler inside `SidebarGitActions.tsx`; driving it through Radix Popover + the menu state machine under jsdom is prohibitively brittle. Contract is verified by code reading + Task 6 manual smoke.)
- **Spec §3.5 (layout):** Task 5 + Task 6 manual smoke.
- **Spec §5 (visual polish):** Divider markup is in Task 3, Step 3. Theme parity covered by Task 6 Step 2.6.
- **Spec §6 (files touched):** Six files listed — five covered by code tasks, sixth (`GitActionsPicker.test.tsx`) covered by Task 2.
- **Spec §7 (testing):** Vitest cases in Tasks 2, 3. Manual smoke in Task 6.
- **Spec §8 (risks):** Collision detection (Radix default — no new code), theme tokens (used), composer discoverability (called out in commit messages).
- **Type consistency:** `variant: 'composer' | 'sidebar'` used identically across Tasks 2, 3. `branchSyncTickAtom` import path `@/atoms/workspace` used identically across Tasks 1, 3, 4, 7. `gitIsRepo` / `gitCurrentBranch` imported from `@/modules/git/api` consistently.

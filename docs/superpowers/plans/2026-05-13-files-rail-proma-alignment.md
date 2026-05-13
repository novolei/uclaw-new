# Files-Rail Proma Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the files-rail panel to mirror Proma's "工作区文件" layout (header + 附加目录 + 工作文件 + footer) and add a per-file hover 3-dot menu with five actions, sharing icon vocabulary with the chat composer's attachment chip.

**Architecture:** `WorkspaceFilesPanel` becomes a 4-block composition (`WorkspacePanelHeader` + `AttachedDirsSection` + `WorkspaceFilesSection` + `WorkspacePanelFooter`). The legacy `MountSection` is deleted; its responsibilities split into `AttachedDirRow` (collapsible row hosting watcher + nested tree) and a flat workspace tree under `WorkspaceFilesSection`. A new `FileRowMenu` hovers over every `FileTreeNode` and wires existing IPCs (`renameArtifact / moveArtifact / deleteArtifactRecursive / reveal_path_in_file_manager / addPendingAttachmentAction`).

**Tech Stack:** React 18 + TypeScript · jotai (existing) · `@react-symbols/icons` (already installed on main) · shadcn `DropdownMenu` + `AlertDialog` + `Tooltip` (already present) · `@tauri-apps/plugin-dialog` (existing) · `sonner` (existing) · Vitest + jsdom.

**Spec:** `docs/superpowers/specs/2026-05-13-files-rail-proma-alignment-design.md` (committed at `792e0cd` on this branch).

**Branch base:** `claude/files-rail-proma-alignment` (off `main` at `38fb5c5`). The plan doc becomes commit 3 of ~17 (spec is commits 1 + 2).

**Zero Rust changes.** Every IPC the menu / footer / header needs already exists. Plan is UI-only.

---

## Pre-flight

- [ ] **Confirm starting state**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout claude/files-rail-proma-alignment
git branch --show-current   # must be claude/files-rail-proma-alignment
git log --oneline -3
# Expect (top to bottom):
#   792e0cd docs(spec): add Section 8a — chat-composer chip icon parity
#   d93189a docs(spec): files-rail Proma alignment design
#   38fb5c5 Merge pull request #127 from novolei/claude/w4d-preview-inline-editing
```

- [ ] **Baseline test counts**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -5
# Expect: Test Files 51 passed (51) · Tests 328 passed (328)
```

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri
cargo test --lib 2>&1 | tail -5
# Expect: 496 passed; 0 failed
```

If either baseline differs, stop and reconcile before starting Task 1.

---

## Task 1: `spaceIdForMount` helper + unit tests

**Files:**
- Create: `ui/src/lib/files-rail-helpers.ts`
- Create: `ui/src/lib/files-rail-helpers.test.ts`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current
# Expect: claude/files-rail-proma-alignment
```

- [ ] **Step 2: Write the failing test**

Create `ui/src/lib/files-rail-helpers.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { spaceIdForMount } from './files-rail-helpers'
import type { MountKind } from '@/atoms/files-rail-atoms'

const mk = (id: string, kind: MountKind = 'workspace') => ({ id, kind })

describe('spaceIdForMount', () => {
  it('extracts space_id from workspace mount', () => {
    expect(spaceIdForMount(mk('workspace:abc123'), null)).toBe('abc123')
  })

  it('extracts space_id from workspace-attached mount (with hash suffix)', () => {
    expect(spaceIdForMount(mk('workspace-attached:abc123:deadbeef0123', 'attached_dir'), null)).toBe('abc123')
  })

  it('falls back to currentWorkspaceId for session mount', () => {
    expect(spaceIdForMount(mk('session:sess-xyz', 'session'), 'fallback-id')).toBe('fallback-id')
  })

  it('falls back to currentWorkspaceId for session-attached mount', () => {
    expect(spaceIdForMount(mk('attached:sess-xyz:cafebabe', 'attached_dir'), 'fallback-id')).toBe('fallback-id')
  })

  it('returns null for session mount when no fallback', () => {
    expect(spaceIdForMount(mk('session:sess-xyz', 'session'), null)).toBeNull()
  })

  it('returns null for malformed id (unknown prefix)', () => {
    expect(spaceIdForMount(mk('totally:bogus'), 'fallback')).toBeNull()
  })

  it('returns null for empty workspace id', () => {
    expect(spaceIdForMount(mk('workspace:'), null)).toBeNull()
  })

  it('returns null for workspace-attached missing colon segment', () => {
    expect(spaceIdForMount(mk('workspace-attached:onlyspace', 'attached_dir'), null)).toBeNull()
  })
})
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/lib/files-rail-helpers.test.ts 2>&1 | tail -10
# Expect: FAIL — "Failed to resolve import" or "spaceIdForMount is not a function"
```

- [ ] **Step 4: Implement `spaceIdForMount`**

Create `ui/src/lib/files-rail-helpers.ts`:

```ts
/**
 * files-rail-helpers — pure utilities shared by files-rail components.
 *
 * `spaceIdForMount` derives the workspace `space_id` from a MountRoot.
 * Encoded in the mount.id for workspace + workspace-attached kinds;
 * falls back to the active workspace atom for session-scoped mounts.
 */

import type { MountKind } from '@/atoms/files-rail-atoms'

export function spaceIdForMount(
  mount: { id: string; kind: MountKind },
  currentWorkspaceId: string | null,
): string | null {
  // workspace:<sid>
  if (mount.id.startsWith('workspace:')) {
    const sid = mount.id.slice('workspace:'.length)
    return sid.length > 0 ? sid : null
  }
  // workspace-attached:<sid>:<hash>
  if (mount.id.startsWith('workspace-attached:')) {
    const rest = mount.id.slice('workspace-attached:'.length)
    const colon = rest.indexOf(':')
    if (colon < 0) return null
    const sid = rest.slice(0, colon)
    return sid.length > 0 ? sid : null
  }
  // session:<sid> or attached:<sid>:<hash> — sessions live in their workspace
  if (mount.id.startsWith('session:') || mount.id.startsWith('attached:')) {
    return currentWorkspaceId
  }
  return null
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/lib/files-rail-helpers.test.ts 2>&1 | tail -10
# Expect: PASS, 8 tests
```

- [ ] **Step 6: Verify branch + commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current
# Expect: claude/files-rail-proma-alignment

git add ui/src/lib/files-rail-helpers.ts ui/src/lib/files-rail-helpers.test.ts
git commit -m "feat(files-rail): spaceIdForMount helper with 8 unit tests

Derives the workspace space_id from a MountRoot for use by the upcoming
3-dot menu's rename/move/delete actions. Parses the mount.id prefix
for workspace + workspace-attached mounts; falls back to the active
workspace atom for session-scoped mounts."

git branch --show-current   # confirm still on claude/files-rail-proma-alignment
git log --oneline -1
```

---

## Task 2: Row-level UI atoms

**Files:**
- Create: `ui/src/atoms/files-rail-row-atoms.ts`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Write the atom module**

Create `ui/src/atoms/files-rail-row-atoms.ts`:

```ts
/**
 * files-rail-row-atoms — single-target atoms for row-level UI state.
 *
 * Only one rename / move / delete can be in flight at a time. The atoms
 * are nullable; setting to non-null opens the corresponding UI (inline
 * rename input, MoveToDialog, DeleteConfirmDialog). Setting back to
 * null closes / commits / cancels.
 *
 * Targets carry the absolute path (canonical identity for files-rail
 * rows) plus enough metadata for the dialogs to render their copy
 * without re-walking the tree.
 */

import { atom } from 'jotai'

export interface FileRowTarget {
  mountId: string
  absolutePath: string
  /** Workspace-relative path (sans leading slash) for IPC calls. */
  workspaceRelPath: string
  name: string
  isDirectory: boolean
}

/** When non-null, the FileTreeNode at this absolutePath renders RenameInput. */
export const renamingFilePathAtom = atom<string | null>(null)

/** When non-null, MoveToDialog is open for this target. */
export const moveTargetAtom = atom<FileRowTarget | null>(null)

/** When non-null, DeleteConfirmDialog is open for this target. */
export const deleteTargetAtom = atom<FileRowTarget | null>(null)
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: no output (clean)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/atoms/files-rail-row-atoms.ts
git commit -m "feat(files-rail): row-level UI atoms (rename / move / delete targets)

Three nullable single-target atoms drive the upcoming inline-rename
input, MoveToDialog, and DeleteConfirmDialog. FileRowTarget carries
both absolute and workspace-relative paths so IPC handlers don't
re-walk the tree to derive them."

git log --oneline -1
```

---

## Task 3: `WorkspacePanelHeader`

**Files:**
- Create: `ui/src/components/files-rail/workspace/WorkspacePanelHeader.tsx`

- [ ] **Step 1: Branch hygiene + read tooltip primitive**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
ls ui/src/components/ui/tooltip.tsx   # must exist
```

- [ ] **Step 2: Implement `WorkspacePanelHeader`**

Create `ui/src/components/files-rail/workspace/WorkspacePanelHeader.tsx`:

```tsx
/**
 * WorkspacePanelHeader — top row of the files-rail workspace panel.
 *
 * [FolderHeart] 工作区文件 [Info ⓘ]            ··· [↻ refresh-all] [↗ Finder]
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { FolderHeart, Info, RotateCw, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  bumpFilesRailRefreshAtom,
  mountRootsAtomFamily,
  fileTreeAtomFamily,
} from '@/atoms/files-rail-atoms'
import { useAtom } from 'jotai'

interface Props {
  sessionId: string | null
  workspaceRootPath: string | null
}

export function WorkspacePanelHeader({ sessionId, workspaceRootPath }: Props): React.ReactElement {
  const bumpRefresh = useSetAtom(bumpFilesRailRefreshAtom)
  const mounts = useAtomValue(mountRootsAtomFamily(sessionId))

  // Spinning while any mount is currently loading.
  // We can't useAtomValue inside a map (rules-of-hooks), so we render a
  // child component per mount and aggregate via React state.
  const [loadingCount, setLoadingCount] = React.useState(0)
  const anyLoading = loadingCount > 0

  const handleRefresh = React.useCallback(() => {
    bumpRefresh()
  }, [bumpRefresh])

  const handleReveal = React.useCallback(async () => {
    if (!workspaceRootPath) return
    try {
      await invoke('reveal_path_in_file_manager', { path: workspaceRootPath })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [workspaceRootPath])

  return (
    <header
      className={cn(
        'flex items-center gap-1.5 flex-shrink-0',
        'h-[36px] px-3',
        'border-b border-border bg-popover',
      )}
    >
      <FolderHeart className="size-3.5 text-muted-foreground shrink-0" />
      <span className="text-[12px] font-medium text-foreground/85">工作区文件</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label="工作区文件说明"
            className="inline-flex items-center justify-center size-4 text-muted-foreground/60 hover:text-muted-foreground transition-colors"
          >
            <Info className="size-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="max-w-[240px]">
          <p className="text-[11px]">工作区内所有会话可访问的文件和文件夹，每个新对话都可以自动读取</p>
        </TooltipContent>
      </Tooltip>
      <div className="flex-1" />
      {mounts.map((m) => (
        <MountLoadProbe
          key={m.id}
          mountId={m.id}
          onLoadingChange={(loading) =>
            setLoadingCount((prev) => prev + (loading ? 1 : -1))
          }
        />
      ))}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={handleRefresh}
            aria-label="刷新文件列表"
            className="inline-flex items-center justify-center size-6 rounded text-muted-foreground/70 hover:text-foreground hover:bg-foreground/[0.06] transition-colors"
          >
            <RotateCw className={cn('size-3.5', anyLoading && 'animate-spin')} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p className="text-[11px]">刷新所有挂载点</p>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={handleReveal}
            disabled={!workspaceRootPath}
            aria-label="在文件管理器中显示工作区"
            className={cn(
              'inline-flex items-center justify-center size-6 rounded transition-colors',
              workspaceRootPath
                ? 'text-muted-foreground/70 hover:text-foreground hover:bg-foreground/[0.06]'
                : 'text-foreground/25 cursor-not-allowed',
            )}
          >
            <ExternalLink className="size-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          <p className="text-[11px]">在文件管理器中显示工作区目录</p>
        </TooltipContent>
      </Tooltip>
    </header>
  )
}

/**
 * Subscribes to one mount's load state and bubbles changes upward via callback.
 * Avoids the hooks-rule violation of calling useAtomValue inside a map.
 */
function MountLoadProbe({
  mountId,
  onLoadingChange,
}: {
  mountId: string
  onLoadingChange: (loading: boolean) => void
}): null {
  const [tree] = useAtom(fileTreeAtomFamily(mountId))
  const loading = tree.status === 'loading'
  const prev = React.useRef(loading)
  React.useEffect(() => {
    if (prev.current !== loading) {
      onLoadingChange(loading)
      prev.current = loading
    }
  }, [loading, onLoadingChange])
  // Drain on unmount so the counter doesn't leak when sessionId changes.
  React.useEffect(() => {
    return () => {
      if (prev.current) onLoadingChange(false)
    }
  }, [onLoadingChange])
  return null
}
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -10
# Expect: clean
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/WorkspacePanelHeader.tsx
git commit -m "feat(files-rail): WorkspacePanelHeader (title + ⓘ + ↻ + ↗)

Top row of the new workspace files panel. Refresh button bumps the
existing filesRailRefreshTickAtom (Task 11 wires useFileTree to it).
Spinning state aggregates per-mount load state via MountLoadProbe to
avoid a hooks-rule violation. ↗ Finder calls reveal_path_in_file_manager
(added in PR #127). Drops the per-mount header — moves to a single
panel-level affordance."

git log --oneline -1
```

---

## Task 4: `WorkspacePanelFooter`

**Files:**
- Create: `ui/src/components/files-rail/workspace/WorkspacePanelFooter.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Implement**

Create `ui/src/components/files-rail/workspace/WorkspacePanelFooter.tsx`:

```tsx
/**
 * WorkspacePanelFooter — two side-by-side action buttons at the bottom of
 * the workspace files panel: 添加文件 (copies a picked file into the
 * workspace) and 附加文件夹 (registers an external dir as a read-only
 * mount). Disabled when no workspace is active.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { Paperclip, FolderPlus } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  attachWorkspaceDirectory,
  copyFileIntoWorkspace,
  openFileDialog,
  openFolderDialog,
} from '@/lib/tauri-bridge'
import { workspaceAttachedDirsMapAtom } from '@/atoms/agent-atoms'

interface Props {
  workspaceId: string | null
}

export function WorkspacePanelFooter({ workspaceId }: Props): React.ReactElement {
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const [busy, setBusy] = React.useState<'addFile' | 'attachDir' | null>(null)

  const handleAddFile = React.useCallback(async () => {
    if (!workspaceId || busy) return
    setBusy('addFile')
    try {
      const result = await openFileDialog()
      if (!result.files || result.files.length === 0) return
      let added = 0
      for (const f of result.files) {
        try {
          // openFileDialog returns objects with a `path` field on Tauri v2.
          const src = (f && typeof f === 'object' && 'path' in f) ? (f as { path: string }).path : String(f)
          await copyFileIntoWorkspace(workspaceId, src)
          added++
        } catch (err) {
          toast.error('文件复制失败', {
            description: err instanceof Error ? err.message : String(err),
          })
        }
      }
      if (added > 0) {
        toast.success(`已添加 ${added} 个文件到工作区`)
      }
    } finally {
      setBusy(null)
    }
  }, [workspaceId, busy])

  const handleAttachDir = React.useCallback(async () => {
    if (!workspaceId || busy) return
    setBusy('attachDir')
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachWorkspaceDirectory(workspaceId, picked.path)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(workspaceId, updated)
        return m
      })
      toast.success(`已附加目录: ${picked.name}`)
    } catch (err) {
      toast.error('附加目录失败', {
        description: err instanceof Error ? err.message : String(err),
      })
    } finally {
      setBusy(null)
    }
  }, [workspaceId, busy, setWsAttachedMap])

  const disabled = !workspaceId
  const disabledTitle = disabled ? '请先选择工作区' : undefined

  return (
    <footer className="flex-shrink-0 grid grid-cols-2 gap-2 p-2 border-t border-border bg-popover">
      <FooterButton
        label="添加文件"
        icon={<Paperclip className="size-4" />}
        onClick={handleAddFile}
        disabled={disabled || busy !== null}
        title={disabledTitle}
      />
      <FooterButton
        label="附加文件夹"
        icon={<FolderPlus className="size-4" />}
        onClick={handleAttachDir}
        disabled={disabled || busy !== null}
        title={disabledTitle}
      />
    </footer>
  )
}

function FooterButton({
  label,
  icon,
  onClick,
  disabled,
  title,
}: {
  label: string
  icon: React.ReactNode
  onClick: () => void
  disabled: boolean
  title?: string
}): React.ReactElement {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cn(
        'flex flex-col items-center justify-center gap-1 py-3',
        'rounded-md border border-dashed border-border bg-foreground/[0.02]',
        'text-[12px] text-muted-foreground',
        'transition-colors',
        !disabled && 'hover:bg-foreground/[0.05] hover:border-border/80 hover:text-foreground',
        disabled && 'opacity-40 cursor-not-allowed',
      )}
    >
      <span>{label}</span>
      {icon}
    </button>
  )
}
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: clean
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/WorkspacePanelFooter.tsx
git commit -m "feat(files-rail): WorkspacePanelFooter (添加文件 / 附加文件夹)

Two grid-50/50 dashed buttons matching Proma's screenshot. 添加文件
calls copyFileIntoWorkspace; 附加文件夹 calls attachWorkspaceDirectory
and pushes the new list to workspaceAttachedDirsMapAtom so the mount
list refetches via the fingerprint dep added in ed0ff02. Both gated on
an active workspace."

git log --oneline -1
```

---

## Task 5: `RenameInput` + validation tests

**Files:**
- Create: `ui/src/components/files-rail/workspace/RenameInput.tsx`
- Create: `ui/src/components/files-rail/workspace/RenameInput.test.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Write failing tests**

Create `ui/src/components/files-rail/workspace/RenameInput.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { RenameInput } from './RenameInput'

describe('RenameInput', () => {
  it('rejects empty name', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('名称不能为空')).toBeTruthy()
  })

  it('rejects separator characters', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'bad/name.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('名称不能包含 / \\ :')).toBeTruthy()
  })

  it('rejects duplicate sibling', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'bar.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(screen.getByText('已存在同名文件')).toBeTruthy()
  })

  it('commits on Enter when valid', async () => {
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.clear(input)
    await user.type(input, 'renamed.ts')
    await user.keyboard('{Enter}')
    expect(onCommit).toHaveBeenCalledWith('renamed.ts')
  })

  it('cancels on Escape', async () => {
    const onCancel = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts'])}
        onCommit={vi.fn()}
        onCancel={onCancel}
      />
    )
    const input = screen.getByRole('textbox')
    await user.click(input)
    await user.keyboard('{Escape}')
    expect(onCancel).toHaveBeenCalled()
  })

  it('does not error when name unchanged (same as initial)', async () => {
    // Editing then reverting should not trigger duplicate-sibling error
    const onCommit = vi.fn()
    const { user } = renderWithProviders(
      <RenameInput
        initialName="foo.ts"
        siblings={new Set(['foo.ts', 'bar.ts'])}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />
    )
    const input = screen.getByRole('textbox')
    await user.click(input)
    await user.keyboard('{Enter}')
    // unchanged → just cancel-equivalent (commit with same name is fine)
    expect(onCommit).toHaveBeenCalledWith('foo.ts')
  })
})
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/components/files-rail/workspace/RenameInput.test.tsx 2>&1 | tail -10
# Expect: FAIL — module not found
```

- [ ] **Step 4: Implement `RenameInput`**

Create `ui/src/components/files-rail/workspace/RenameInput.tsx`:

```tsx
/**
 * RenameInput — inline rename field for files-rail rows.
 *
 * Replaces the row's filename <span> while active. Synchronous
 * validation (empty / separator chars / duplicate sibling) shows an
 * inline error; Enter commits when valid, Escape cancels, blur
 * commits-or-cancels based on error state.
 */

import * as React from 'react'
import { cn } from '@/lib/utils'

interface Props {
  initialName: string
  /** Names of every sibling at the same depth (used to detect dup). */
  siblings: Set<string>
  onCommit: (newName: string) => void
  onCancel: () => void
}

const SEPARATOR_CHARS = /[/\\:]/

function validate(value: string, initialName: string, siblings: Set<string>): string | null {
  const trimmed = value.trim()
  if (trimmed.length === 0) return '名称不能为空'
  if (SEPARATOR_CHARS.test(trimmed)) return '名称不能包含 / \\ :'
  if (trimmed !== initialName && siblings.has(trimmed)) return '已存在同名文件'
  return null
}

export function RenameInput({ initialName, siblings, onCommit, onCancel }: Props): React.ReactElement {
  const [value, setValue] = React.useState(initialName)
  const [error, setError] = React.useState<string | null>(null)
  const inputRef = React.useRef<HTMLInputElement>(null)

  // Auto-focus + select basename (preserve extension).
  React.useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.focus()
    const dot = initialName.lastIndexOf('.')
    if (dot > 0) {
      el.setSelectionRange(0, dot)
    } else {
      el.select()
    }
  }, [initialName])

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
    const next = e.target.value
    setValue(next)
    setError(validate(next, initialName, siblings))
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === 'Enter') {
      e.preventDefault()
      const err = validate(value, initialName, siblings)
      if (err) {
        setError(err)
        return
      }
      onCommit(value.trim())
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
    }
  }

  const handleBlur = (): void => {
    const err = validate(value, initialName, siblings)
    if (err) {
      onCancel()
    } else {
      onCommit(value.trim())
    }
  }

  return (
    <div className="flex-1 min-w-0">
      <input
        ref={inputRef}
        type="text"
        role="textbox"
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'w-full bg-transparent text-[12px] border-b outline-none py-0.5 px-0',
          error ? 'border-destructive' : 'border-primary/50',
        )}
        maxLength={255}
      />
      {error && (
        <div className="text-[10px] text-destructive mt-0.5 truncate">{error}</div>
      )}
    </div>
  )
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/components/files-rail/workspace/RenameInput.test.tsx 2>&1 | tail -10
# Expect: PASS, 6 tests
```

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/RenameInput.tsx ui/src/components/files-rail/workspace/RenameInput.test.tsx
git commit -m "feat(files-rail): RenameInput inline editor + 6 validation tests

Auto-focus + basename selection. Synchronous validation rejects empty,
separator chars (/\\:), and duplicate siblings (Set-backed O(1)).
Enter commits when valid, Escape cancels, blur commits-or-cancels."

git log --oneline -1
```

---

## Task 6: `DeleteConfirmDialog`

**Files:**
- Create: `ui/src/components/files-rail/workspace/DeleteConfirmDialog.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Implement**

Create `ui/src/components/files-rail/workspace/DeleteConfirmDialog.tsx`:

```tsx
/**
 * DeleteConfirmDialog — shadcn AlertDialog wrapping deleteArtifactRecursive.
 *
 * Driven by deleteTargetAtom: when non-null the dialog is open. Confirm
 * fires the IPC; on success clears the atom + toasts; on error keeps
 * the dialog open with the error message.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { deleteArtifactRecursive } from '@/lib/tauri-bridge'
import { deleteTargetAtom } from '@/atoms/files-rail-row-atoms'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import type { MountKind } from '@/atoms/files-rail-atoms'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'

interface Props {
  /** mountKind for the current target — used by spaceIdForMount. */
  mountKindForTarget?: MountKind
  /** Called after a successful delete so the panel can refetch the parent. */
  onDeleted?: (target: { mountId: string; absolutePath: string }) => void
}

export function DeleteConfirmDialog({ mountKindForTarget = 'workspace', onDeleted }: Props): React.ReactElement {
  const [target, setTarget] = useAtom(deleteTargetAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const [submitting, setSubmitting] = React.useState(false)
  const [submitError, setSubmitError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!target) setSubmitError(null)
  }, [target])

  const handleCancel = React.useCallback(() => {
    if (submitting) return
    setTarget(null)
  }, [submitting, setTarget])

  const handleConfirm = React.useCallback(async () => {
    if (!target) return
    const spaceId = spaceIdForMount({ id: target.mountId, kind: mountKindForTarget }, currentWorkspaceId)
    if (!spaceId) {
      setSubmitError('无法解析工作区 ID')
      return
    }
    setSubmitting(true)
    setSubmitError(null)
    try {
      await deleteArtifactRecursive(spaceId, target.workspaceRelPath)
      toast.success(`已删除 ${target.name}`)
      onDeleted?.({ mountId: target.mountId, absolutePath: target.absolutePath })
      setTarget(null)
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err))
    } finally {
      setSubmitting(false)
    }
  }, [target, currentWorkspaceId, mountKindForTarget, onDeleted, setTarget])

  const open = target !== null

  return (
    <AlertDialog open={open} onOpenChange={(o) => { if (!o) handleCancel() }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>确认删除</AlertDialogTitle>
          <AlertDialogDescription>
            {target && (
              <>
                确定要删除 <strong className="text-foreground">{target.name}</strong> 吗？此操作不可撤销。
                {target.isDirectory && <span className="text-muted-foreground">（包含其下全部内容）</span>}
              </>
            )}
            {submitError && (
              <span className="block mt-2 text-destructive text-[11px]">{submitError}</span>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={submitting} onClick={handleCancel}>取消</AlertDialogCancel>
          <AlertDialogAction
            disabled={submitting}
            onClick={(e) => { e.preventDefault(); void handleConfirm() }}
            className={cn('bg-destructive text-destructive-foreground hover:bg-destructive/90')}
          >
            {submitting ? '删除中…' : '删除'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: clean
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/DeleteConfirmDialog.tsx
git commit -m "feat(files-rail): DeleteConfirmDialog backed by deleteTargetAtom

Opens when the atom is non-null. Confirm fires
deleteArtifactRecursive(space_id, workspace_rel_path); on error the
dialog stays open with an inline message so the user can retry. onDeleted
callback lets the caller refetch the parent directory after success."

git log --oneline -1
```

---

## Task 7: `MoveToDialog`

**Files:**
- Create: `ui/src/components/files-rail/workspace/MoveToDialog.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Implement**

Create `ui/src/components/files-rail/workspace/MoveToDialog.tsx`:

```tsx
/**
 * MoveToDialog — opens the OS folder picker, validates the choice falls
 * inside the workspace dir, then calls moveArtifact.
 *
 * No modal UI of our own — we delegate to @tauri-apps/plugin-dialog.
 * This component is rendered once at the rail level; it reacts to
 * moveTargetAtom transitioning non-null by opening the picker.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { toast } from 'sonner'
import { moveArtifact, openFolderDialog } from '@/lib/tauri-bridge'
import { moveTargetAtom } from '@/atoms/files-rail-row-atoms'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import type { MountKind } from '@/atoms/files-rail-atoms'

interface Props {
  /** Workspace root absolute path (used to validate the picked dir). */
  workspaceRootPath: string | null
  /** mountKind for the target — used by spaceIdForMount. Defaults to 'workspace'. */
  mountKindForTarget?: MountKind
  /** Called after a successful move so the caller can refetch source + dest parents. */
  onMoved?: (info: { mountId: string; srcAbsolutePath: string; destAbsolutePath: string }) => void
}

export function MoveToDialog({
  workspaceRootPath,
  mountKindForTarget = 'workspace',
  onMoved,
}: Props): null {
  const [target, setTarget] = useAtom(moveTargetAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const runningRef = React.useRef(false)

  React.useEffect(() => {
    if (!target || runningRef.current) return
    runningRef.current = true

    void (async () => {
      try {
        const picked = await openFolderDialog()
        if (!picked) {
          return
        }
        if (!workspaceRootPath) {
          toast.error('工作区路径未解析，无法移动')
          return
        }
        const wsRoot = workspaceRootPath.replace(/\/+$/, '')
        if (picked.path !== wsRoot && !picked.path.startsWith(wsRoot + '/')) {
          toast.error('只能移动到当前工作区内的文件夹')
          return
        }
        const spaceId = spaceIdForMount({ id: target.mountId, kind: mountKindForTarget }, currentWorkspaceId)
        if (!spaceId) {
          toast.error('无法解析工作区 ID')
          return
        }
        // destPath is workspace-relative, joined with the original basename.
        const destRelDir = picked.path === wsRoot ? '' : picked.path.slice(wsRoot.length + 1)
        const destRelPath = destRelDir.length > 0 ? `${destRelDir}/${target.name}` : target.name
        if (destRelPath === target.workspaceRelPath) {
          // No-op — picked the existing parent.
          return
        }
        try {
          await moveArtifact({
            spaceId,
            srcPath: target.workspaceRelPath,
            destPath: destRelPath,
          })
          toast.success(`已移动 ${target.name}`)
          const destAbsolute = `${wsRoot}/${destRelPath}`
          onMoved?.({
            mountId: target.mountId,
            srcAbsolutePath: target.absolutePath,
            destAbsolutePath: destAbsolute,
          })
        } catch (err) {
          toast.error('移动失败', {
            description: err instanceof Error ? err.message : String(err),
          })
        }
      } finally {
        runningRef.current = false
        setTarget(null)
      }
    })()
  }, [target, workspaceRootPath, currentWorkspaceId, mountKindForTarget, onMoved, setTarget])

  return null
}
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: clean
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/MoveToDialog.tsx
git commit -m "feat(files-rail): MoveToDialog driven by moveTargetAtom

Setting the atom triggers the OS folder picker via openFolderDialog.
Picked path must live under the workspace root — anything outside
toasts an error. Computes the workspace-relative destPath and calls
moveArtifact. No custom modal UI; defers to the native picker per the
spec's YAGNI cut."

git log --oneline -1
```

---

## Task 8: `FileRowMenu` + render tests

**Files:**
- Create: `ui/src/components/files-rail/workspace/FileRowMenu.tsx`
- Create: `ui/src/components/files-rail/workspace/FileRowMenu.test.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Write failing tests**

Create `ui/src/components/files-rail/workspace/FileRowMenu.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { FileRowMenu } from './FileRowMenu'
import type { MountRoot } from '@/atoms/files-rail-atoms'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  convertFileSrc: (s: string) => s,
}))

const wsMount: MountRoot = {
  id: 'workspace:abc',
  label: 'Workspace',
  path: '/ws/root',
  kind: 'workspace',
  editable: true,
}
const attachedMount: MountRoot = {
  id: 'workspace-attached:abc:hash123',
  label: 'External',
  path: '/external/dir',
  kind: 'attached_dir',
  editable: false,
}

describe('FileRowMenu', () => {
  it('renders all 5 items enabled on a workspace mount file', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={wsMount}
        sessionId="sess-1"
        relPath="sub/foo.ts"
        name="foo.ts"
        isDirectory={false}
        absolutePath="/ws/root/sub/foo.ts"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    expect(screen.getByText('添加到聊天')).toBeTruthy()
    expect(screen.getByText('在文件夹中显示')).toBeTruthy()
    expect(screen.getByText('移动到…')).toBeTruthy()
    expect(screen.getByText('重命名')).toBeTruthy()
    expect(screen.getByText('删除')).toBeTruthy()
    // None should have data-disabled
    const move = screen.getByText('移动到…').closest('[role="menuitem"]')!
    expect(move.getAttribute('data-disabled')).toBeNull()
  })

  it('disables 移动到… / 重命名 / 删除 on a read-only attached mount', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={attachedMount}
        sessionId="sess-1"
        relPath="img.png"
        name="img.png"
        isDirectory={false}
        absolutePath="/external/dir/img.png"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    const move = screen.getByText('移动到…').closest('[role="menuitem"]')!
    const rename = screen.getByText('重命名').closest('[role="menuitem"]')!
    const del = screen.getByText('删除').closest('[role="menuitem"]')!
    expect(move.getAttribute('data-disabled')).not.toBeNull()
    expect(rename.getAttribute('data-disabled')).not.toBeNull()
    expect(del.getAttribute('data-disabled')).not.toBeNull()
    // Items 1 + 2 stay enabled
    const addToChat = screen.getByText('添加到聊天').closest('[role="menuitem"]')!
    const reveal = screen.getByText('在文件夹中显示').closest('[role="menuitem"]')!
    expect(addToChat.getAttribute('data-disabled')).toBeNull()
    expect(reveal.getAttribute('data-disabled')).toBeNull()
  })

  it('hides 添加到聊天 for directories', async () => {
    const { user } = renderWithProviders(
      <FileRowMenu
        mount={wsMount}
        sessionId="sess-1"
        relPath="sub"
        name="sub"
        isDirectory={true}
        absolutePath="/ws/root/sub"
      />
    )
    await user.click(screen.getByRole('button', { name: '更多操作' }))
    expect(screen.queryByText('添加到聊天')).toBeNull()
  })
})
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/components/files-rail/workspace/FileRowMenu.test.tsx 2>&1 | tail -10
# Expect: FAIL — module not found
```

- [ ] **Step 4: Implement `FileRowMenu`**

Create `ui/src/components/files-rail/workspace/FileRowMenu.tsx`:

```tsx
/**
 * FileRowMenu — 3-dot hover menu for a single FileTreeNode row.
 *
 * Five actions surface via shadcn DropdownMenu:
 *   1. 添加到聊天 (files only)            — addPendingAttachmentAction
 *   2. 在文件夹中显示                     — reveal_path_in_file_manager
 *   3. 移动到…   (workspace-mount only)  — moveTargetAtom
 *   4. 重命名     (workspace-mount only)  — renamingFilePathAtom
 *   5. 删除       (workspace-mount only)  — deleteTargetAtom
 *
 * Items 3/4/5 render but disabled on non-workspace mounts (clear UX
 * over hidden actions). Backend rename/move/delete commands resolve
 * paths under <data_dir>/spaces/<sid>/workspace/ only — they literally
 * cannot operate on attached or session paths.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import {
  MoreHorizontal,
  MessageSquarePlus,
  FolderSearch,
  FolderInput,
  Pencil,
  Trash2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { addPendingAttachmentAction } from '@/atoms/preview-chip-atoms'
import {
  renamingFilePathAtom,
  moveTargetAtom,
  deleteTargetAtom,
} from '@/atoms/files-rail-row-atoms'
import type { MountRoot } from '@/atoms/files-rail-atoms'

interface Props {
  mount: MountRoot
  sessionId: string | null
  relPath: string
  name: string
  isDirectory: boolean
  absolutePath: string
}

const READONLY_TOOLTIP = '只读 — 编辑此挂载点需要批准'

export function FileRowMenu({
  mount,
  sessionId,
  relPath,
  name,
  isDirectory,
  absolutePath,
}: Props): React.ReactElement {
  const addAttachment = useSetAtom(addPendingAttachmentAction)
  const setRenaming = useSetAtom(renamingFilePathAtom)
  const setMoveTarget = useSetAtom(moveTargetAtom)
  const setDeleteTarget = useSetAtom(deleteTargetAtom)

  const isMutable = mount.kind === 'workspace'

  const handleAddToChat = React.useCallback(() => {
    void addAttachment({
      mountId: mount.id,
      relPath,
      name,
      sessionId,
      absolutePath,
    })
  }, [addAttachment, mount.id, relPath, name, sessionId, absolutePath])

  const handleReveal = React.useCallback(async () => {
    try {
      await invoke('reveal_path_in_file_manager', { path: absolutePath })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [absolutePath])

  const handleMove = React.useCallback(() => {
    setMoveTarget({
      mountId: mount.id,
      absolutePath,
      workspaceRelPath: relPath,
      name,
      isDirectory,
    })
  }, [setMoveTarget, mount.id, absolutePath, relPath, name, isDirectory])

  const handleRename = React.useCallback(() => {
    setRenaming(absolutePath)
  }, [setRenaming, absolutePath])

  const handleDelete = React.useCallback(() => {
    setDeleteTarget({
      mountId: mount.id,
      absolutePath,
      workspaceRelPath: relPath,
      name,
      isDirectory,
    })
  }, [setDeleteTarget, mount.id, absolutePath, relPath, name, isDirectory])

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="更多操作"
          title="更多操作"
          onClick={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
          className={cn(
            'size-6 rounded inline-flex items-center justify-center shrink-0',
            'text-muted-foreground hover:text-foreground hover:bg-accent/70',
            'invisible group-hover/row:visible focus-visible:visible data-[state=open]:visible',
            'transition-colors',
          )}
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-44 z-[9999] min-w-0 p-0.5">
        {!isDirectory && (
          <DropdownMenuItem
            className="text-xs py-1 [&>svg]:size-3.5"
            onSelect={handleAddToChat}
          >
            <MessageSquarePlus />
            添加到聊天
          </DropdownMenuItem>
        )}
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          onSelect={() => void handleReveal()}
        >
          <FolderSearch />
          在文件夹中显示
        </DropdownMenuItem>
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleMove()
          }}
        >
          <FolderInput />
          移动到…
        </DropdownMenuItem>
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleRename()
          }}
        >
          <Pencil />
          重命名
        </DropdownMenuItem>
        <DropdownMenuSeparator className="my-0.5" />
        <DropdownMenuItem
          className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
          disabled={!isMutable}
          title={!isMutable ? READONLY_TOOLTIP : undefined}
          onSelect={(e) => {
            if (!isMutable) {
              e.preventDefault()
              return
            }
            handleDelete()
          }}
        >
          <Trash2 />
          删除
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx vitest run src/components/files-rail/workspace/FileRowMenu.test.tsx 2>&1 | tail -10
# Expect: PASS, 3 tests
```

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/FileRowMenu.tsx ui/src/components/files-rail/workspace/FileRowMenu.test.tsx
git commit -m "feat(files-rail): FileRowMenu (3-dot hover menu w/ 5 actions)

Trigger appears on row hover (invisible group-hover:visible). Items
add-to-chat / reveal stay enabled on every mount; move / rename / delete
disable + tooltip on non-workspace mounts (backend artifact IPCs only
resolve paths under the workspace dir). Three rendering tests cover
workspace-mount full menu, read-only attached-dir gating, and the
add-to-chat hide rule for directories."

git log --oneline -1
```

---

## Task 9: Augment `FileTreeNode` with menu + inline rename (backward-compatible)

**Files:**
- Modify: `ui/src/components/files-rail/workspace/FileTreeNode.tsx`

**Backward-compatibility note:** The new props (`mount`, `sessionId`, `siblings`, `onRenamed`) are **optional** in this task. Existing `MountSection` callers continue to work unchanged (no menu, no rename) until Task 13 deletes `MountSection` and tightens the props back to required. This keeps every commit between Task 9 and Task 13 building cleanly.

- [ ] **Step 1: Branch hygiene + re-read current state**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
cat ui/src/components/files-rail/workspace/FileTreeNode.tsx | head -20
# Confirm current 83-line implementation (no menu, no rename).
```

- [ ] **Step 2: Replace file with menu + rename-aware implementation (optional new props)**

Overwrite `ui/src/components/files-rail/workspace/FileTreeNode.tsx`:

```tsx
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import { FileRowMenu } from './FileRowMenu'
import { RenameInput } from './RenameInput'
import {
  renamingFilePathAtom,
} from '@/atoms/files-rail-row-atoms'
import { renameArtifact } from '@/lib/tauri-bridge'
import { spaceIdForMount } from '@/lib/files-rail-helpers'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'
import type { MountRoot } from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
  /** Mount this node belongs to — drives menu gating and IPC routing. Optional
   *  for backward-compat with legacy callers (MountSection); when absent, the
   *  row renders without the 3-dot menu. Task 13 tightens this to required. */
  mount?: MountRoot
  /** Active session ID (for addPendingAttachmentAction). Optional, see above. */
  sessionId?: string | null
  /** Sibling names at this depth — used by RenameInput for dup detection. */
  siblings?: Set<string>
  /** Called after a successful rename so the panel can refetch. */
  onRenamed?: (info: { mountId: string; oldRelPath: string; newRelPath: string }) => void
}

export const FileTreeNode = React.memo(function FileTreeNode({
  node,
  depth,
  isExpanded,
  onToggle,
  onFileClick,
  mount,
  sessionId,
  siblings,
  onRenamed,
}: FileTreeNodeProps): React.ReactElement {
  const expanded = isExpanded(node.relPath)
  const isDir = node.kind === 'directory'
  // Legacy MountSection callers don't pass `mount` — skip the menu/rename code
  // path entirely so behaviour matches the pre-Task-9 file.
  const hasMenuContext = mount !== undefined
  const absolutePath = mount ? `${mount.path}/${node.relPath}` : ''
  const renamingPath = useAtomValue(renamingFilePathAtom)
  const setRenaming = useSetAtom(renamingFilePathAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const isRenaming = hasMenuContext && renamingPath === absolutePath

  const handleClick = React.useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      if (isRenaming) return
      if (isDir) void onToggle(node.relPath, true)
      else onFileClick(node, event)
    },
    [isDir, isRenaming, node, onToggle, onFileClick],
  )

  const handleRenameCommit = React.useCallback(async (newName: string) => {
    if (!mount) return
    if (newName === node.name) {
      setRenaming(null)
      return
    }
    const spaceId = spaceIdForMount(mount, currentWorkspaceId)
    if (!spaceId) {
      toast.error('无法解析工作区 ID')
      setRenaming(null)
      return
    }
    const parts = node.relPath.split('/')
    parts[parts.length - 1] = newName
    const newRelPath = parts.join('/')
    try {
      await renameArtifact({
        spaceId,
        oldPath: node.relPath,
        newPath: newRelPath,
      })
      toast.success(`已重命名为 ${newName}`)
      onRenamed?.({ mountId: mount.id, oldRelPath: node.relPath, newRelPath })
      setRenaming(null)
    } catch (err) {
      toast.error('重命名失败', {
        description: err instanceof Error ? err.message : String(err),
      })
      // leave rename open so user can retry
    }
  }, [node.name, node.relPath, mount, currentWorkspaceId, onRenamed, setRenaming])

  const handleRenameCancel = React.useCallback(() => {
    setRenaming(null)
  }, [setRenaming])

  const indent = depth * 12

  return (
    <>
      <div
        className={cn(
          'group/row relative flex items-center w-full h-[22px] px-2 gap-1',
          'text-[12px] text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
        )}
        style={{ paddingLeft: 8 + indent }}
      >
        <button
          type="button"
          onClick={handleClick}
          className={cn(
            'flex-1 min-w-0 flex items-center gap-1 text-left h-full',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          )}
          title={node.relPath}
        >
          {isDir ? (
            expanded ? (
              <ChevronDown size={12} className="shrink-0 text-foreground/40" />
            ) : (
              <ChevronRight size={12} className="shrink-0 text-foreground/40" />
            )
          ) : (
            <span className="w-3 shrink-0" aria-hidden />
          )}
          <FileTypeIcon
            name={node.name}
            isDirectory={isDir}
            isOpen={isDir && expanded}
            size={14}
            className="shrink-0"
          />
          {isRenaming && mount && siblings ? (
            <RenameInput
              initialName={node.name}
              siblings={siblings}
              onCommit={(newName) => void handleRenameCommit(newName)}
              onCancel={handleRenameCancel}
            />
          ) : (
            <span className="truncate font-mono tabular-nums">{node.name}</span>
          )}
        </button>
        {hasMenuContext && mount && !isRenaming && (
          <FileRowMenu
            mount={mount}
            sessionId={sessionId ?? null}
            relPath={node.relPath}
            name={node.name}
            isDirectory={isDir}
            absolutePath={absolutePath}
          />
        )}
      </div>
      {isDir && expanded && node.children && (
        <>
          {(() => {
            const childSiblings = new Set(node.children.map((c) => c.name))
            return node.children.map((child) => (
              <FileTreeNode
                key={child.relPath}
                node={child}
                depth={depth + 1}
                isExpanded={isExpanded}
                onToggle={onToggle}
                onFileClick={onFileClick}
                mount={mount}
                sessionId={sessionId}
                siblings={childSiblings}
                onRenamed={onRenamed}
              />
            ))
          })()}
        </>
      )}
    </>
  )
})
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -10
# Expect: clean. Legacy MountSection callers continue to work because all new
# props are optional.
```

- [ ] **Step 4: Run UI tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -5
# Expect: still 328 + 8 + 6 + 3 = 345 passing (Tasks 1, 5, 8 already added 17).
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/FileTreeNode.tsx
git commit -m "feat(files-rail): FileTreeNode supports menu + inline rename (optional)

New optional props (mount, sessionId, siblings, onRenamed) wire up the
3-dot FileRowMenu + RenameInput when supplied. Legacy MountSection
callers don't pass them and continue rendering today's bare row. Task
13 tightens the props to required after MountSection is deleted."

git log --oneline -1
```

---

## Task 10: `AttachedDirRow`

**Files:**
- Create: `ui/src/components/files-rail/workspace/AttachedDirRow.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Implement**

Create `ui/src/components/files-rail/workspace/AttachedDirRow.tsx`:

```tsx
/**
 * AttachedDirRow — one collapsible row per attached-dir mount.
 *
 * - Chevron + folder icon + label + Lock badge (when !editable).
 * - 3-dot menu on the row shows only 在文件夹中显示 (mount-label rename
 *   is out of scope per spec § Section 2).
 * - When expanded, mounts the watcher (filesRailWatchStart) and
 *   recursively renders FileTreeNode children. Collapsing unregisters
 *   the watcher.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { ChevronRight, Lock, MoreHorizontal, FolderSearch } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import { FileTreeNode } from './FileTreeNode'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { useFilesRailWatcher } from '@/components/files-rail/hooks/useFilesRailWatcher'
import { filesRailWatchStart, filesRailWatchStop } from '@/lib/tauri-bridge'
import {
  expandedPathsAtomFamily,
  type MountRoot,
} from '@/atoms/files-rail-atoms'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface Props {
  mount: MountRoot
  sessionId: string | null
  onFileClick: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

const TOP_EXPAND_KEY = '__top__'  // sentinel for the row's own collapse state

export function AttachedDirRow({ mount, sessionId, onFileClick }: Props): React.ReactElement {
  const [expanded, setExpanded] = useAtom(expandedPathsAtomFamily(mount.id))
  const isExpanded = expanded.has(TOP_EXPAND_KEY)
  const treeApi = useFileTree(mount.id, sessionId)
  useFilesRailWatcher(mount.id, treeApi.applyExternalChanges)

  // Mount the OS watcher only while expanded.
  React.useEffect(() => {
    if (!isExpanded) return
    void filesRailWatchStart(mount.id, sessionId).catch(() => { /* silent */ })
    return () => {
      void filesRailWatchStop(mount.id).catch(() => { /* idempotent */ })
    }
  }, [isExpanded, mount.id, sessionId])

  const toggleTop = React.useCallback(() => {
    const next = new Set(expanded)
    if (next.has(TOP_EXPAND_KEY)) next.delete(TOP_EXPAND_KEY)
    else next.add(TOP_EXPAND_KEY)
    setExpanded(next)
  }, [expanded, setExpanded])

  const handleReveal = React.useCallback(async () => {
    try {
      await invoke('reveal_path_in_file_manager', { path: mount.path })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [mount.path])

  const isChildExpanded = React.useCallback(
    (rel: string) => expanded.has(rel),
    [expanded],
  )

  const handleChildToggle = React.useCallback(
    async (rel: string, isDir: boolean) => {
      // Delegate to the useFileTree's toggleExpand which handles lazy load
      await treeApi.toggleExpand(rel, isDir)
    },
    [treeApi],
  )

  const handleChildFileClick = React.useCallback(
    (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => onFileClick(mount, node, event),
    [mount, onFileClick],
  )

  const topSiblings = React.useMemo(
    () => new Set(treeApi.nodes.map((n) => n.name)),
    [treeApi.nodes],
  )

  return (
    <section>
      <div className="group/row relative flex items-center w-full h-[24px] px-2 gap-1 text-[12px] text-foreground/90 hover:bg-foreground/[0.04] transition-colors">
        <button
          type="button"
          onClick={toggleTop}
          className="flex-1 min-w-0 flex items-center gap-1 text-left h-full focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          title={mount.path}
        >
          <ChevronRight
            size={12}
            className={cn(
              'shrink-0 text-foreground/50 transition-transform duration-150',
              isExpanded && 'rotate-90',
            )}
          />
          <FileTypeIcon name={mount.label} isDirectory size={14} className="shrink-0" />
          <span className="truncate font-medium">{mount.label}</span>
          {!mount.editable && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Lock className="size-2.5 text-muted-foreground/60 shrink-0" aria-label="只读" />
              </TooltipTrigger>
              <TooltipContent side="bottom">
                <p className="text-[11px]">只读 — 编辑此挂载点需要批准</p>
              </TooltipContent>
            </Tooltip>
          )}
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="更多操作"
              title="更多操作"
              onClick={(e) => e.stopPropagation()}
              onMouseDown={(e) => e.stopPropagation()}
              className={cn(
                'size-6 rounded inline-flex items-center justify-center shrink-0',
                'text-muted-foreground hover:text-foreground hover:bg-accent/70',
                'invisible group-hover/row:visible focus-visible:visible data-[state=open]:visible',
                'transition-colors',
              )}
            >
              <MoreHorizontal className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-44 z-[9999] min-w-0 p-0.5">
            <DropdownMenuItem
              className="text-xs py-1 [&>svg]:size-3.5"
              onSelect={() => void handleReveal()}
            >
              <FolderSearch />
              在文件夹中显示
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {isExpanded && treeApi.loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive truncate">
          {treeApi.errorMessage ?? '加载失败'}
        </div>
      )}
      {isExpanded && treeApi.loadState === 'ready' && treeApi.nodes.length === 0 && (
        <div className="px-3 py-2 text-[11px] text-muted-foreground/70">空文件夹</div>
      )}
      {isExpanded && treeApi.nodes.length > 0 && (
        <div className="min-h-0">
          {treeApi.nodes.map((child) => (
            <FileTreeNode
              key={child.relPath}
              node={child}
              depth={1}
              isExpanded={isChildExpanded}
              onToggle={handleChildToggle}
              onFileClick={handleChildFileClick}
              mount={mount}
              sessionId={sessionId}
              siblings={topSiblings}
            />
          ))}
        </div>
      )}
    </section>
  )
}
```

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -10
# Expect: clean. AttachedDirRow uses the new optional props on FileTreeNode
# (Task 9). MountSection still uses the legacy call signature and remains valid.
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/workspace/AttachedDirRow.tsx
git commit -m "feat(files-rail): AttachedDirRow (collapsible + watcher-gated)

One per attached mount. ChevronRight rotates 90° when expanded.
Watcher (filesRailWatchStart) only runs while expanded — collapsing
stops FS notifications to save resources. The row's own 3-dot menu
offers only 在文件夹中显示; the full menu is on each nested file via
FileTreeNode. Lock tooltip surfaces the read-only reason."

git log --oneline -1
```

---

## Task 11: `WorkspaceFilesPanel` rewrite (new composition + watcher refresh wiring)

**Files:**
- Modify: `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx`
- Modify: `ui/src/components/files-rail/hooks/useFileTree.ts` (subscribe to refresh tick)

- [ ] **Step 1: Branch hygiene + read useFileTree current state**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
cat ui/src/components/files-rail/hooks/useFileTree.ts | sed -n '47,49p'
# Confirm line 47-49 contains the useEffect that triggers reload on idle.
```

- [ ] **Step 2: Wire `filesRailRefreshTickAtom` into `useFileTree`**

Modify `ui/src/components/files-rail/hooks/useFileTree.ts` — add the tick subscription so the header ↻ button can reload all visible mounts.

Find this block:

```ts
import {
  expandedPathsAtomFamily,
  fileTreeAtomFamily,
} from '@/atoms/files-rail-atoms'
```

Replace with:

```ts
import { useAtomValue } from 'jotai'
import {
  expandedPathsAtomFamily,
  fileTreeAtomFamily,
  filesRailRefreshTickAtom,
} from '@/atoms/files-rail-atoms'
```

Find this block:

```ts
  React.useEffect(() => {
    if (tree.status === 'idle') void reload()
  }, [tree.status, reload])
```

Replace with:

```ts
  // Header ↻ button bumps filesRailRefreshTickAtom; observe it so every
  // visible mount refetches when the tick changes (idle reload remains
  // the trigger for first-mount fetches).
  const refreshTick = useAtomValue(filesRailRefreshTickAtom)
  const prevTickRef = React.useRef(refreshTick)
  React.useEffect(() => {
    if (tree.status === 'idle') {
      void reload()
      return
    }
    if (refreshTick !== prevTickRef.current) {
      prevTickRef.current = refreshTick
      void reload()
    }
  }, [tree.status, reload, refreshTick])
```

- [ ] **Step 3: Rewrite `WorkspaceFilesPanel.tsx`**

Overwrite `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx`:

```tsx
/**
 * WorkspaceFilesPanel — the Files tab's content for the workspace tab.
 *
 * Composition (top to bottom):
 *   - WorkspacePanelHeader
 *   - AttachedDirsSection  (subtitle + AttachedDirRow × N, only when any)
 *   - WorkspaceFilesSection (subtitle + flat FileTreeNode tree)
 *   - WorkspacePanelFooter (添加文件 / 附加文件夹)
 *
 * The three single-target dialogs (MoveToDialog / DeleteConfirmDialog +
 * the inline RenameInput inside FileTreeNode) are mounted here at panel
 * scope so atom transitions surface their UI regardless of which row
 * triggered them.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { mountRootsAtomFamily, type MountRoot } from '@/atoms/files-rail-atoms'
import {
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
  currentAgentWorkspaceIdAtom,
} from '@/atoms/agent-atoms'
import { filesRailListMounts } from '@/lib/tauri-bridge'
import { useFileTree } from '@/components/files-rail/hooks/useFileTree'
import { useFilesRailWatcher } from '@/components/files-rail/hooks/useFilesRailWatcher'
import { filesRailWatchStart, filesRailWatchStop } from '@/lib/tauri-bridge'
import { FileTreeNode } from './FileTreeNode'
import { AttachedDirRow } from './AttachedDirRow'
import { WorkspacePanelHeader } from './WorkspacePanelHeader'
import { WorkspacePanelFooter } from './WorkspacePanelFooter'
import { MoveToDialog } from './MoveToDialog'
import { DeleteConfirmDialog } from './DeleteConfirmDialog'
import type { TreeNode } from '@/components/files-rail/utils/tree-patch'

interface WorkspaceFilesPanelProps {
  sessionId: string | null
  onFileClick?: (mount: MountRoot, node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
}

function fingerprintAttachedDirs(
  wsMap: Map<string, string[]>,
  sessionMap: Map<string, string[]>,
): string {
  const wsEntries = Array.from(wsMap.entries())
    .map(([k, v]) => `${k}:${v.join(',')}`)
    .sort()
  const sessionEntries = Array.from(sessionMap.entries())
    .map(([k, v]) => `${k}:${v.join(',')}`)
    .sort()
  return `${wsEntries.join('|')}#${sessionEntries.join('|')}`
}

export function WorkspaceFilesPanel({
  sessionId,
  onFileClick,
}: WorkspaceFilesPanelProps): React.ReactElement {
  const [mounts, setMounts] = useAtom(mountRootsAtomFamily(sessionId))
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const attachedFingerprint = fingerprintAttachedDirs(wsAttachedMap, sessionAttachedMap)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)

  React.useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const fetched = await filesRailListMounts(sessionId)
        if (!cancelled) setMounts(fetched)
      } catch {
        if (!cancelled) setMounts([])
      }
    })()
    return () => { cancelled = true }
  }, [sessionId, attachedFingerprint, setMounts])

  const workspaceMount = mounts.find((m) => m.kind === 'workspace') ?? null
  const attachedMounts = mounts.filter((m) => m.kind === 'attached_dir')
  const workspaceRootPath = workspaceMount?.path ?? null

  return (
    <div className="flex flex-col h-full min-h-0">
      <WorkspacePanelHeader
        sessionId={sessionId}
        workspaceRootPath={workspaceRootPath}
      />
      <div className="flex-1 min-h-0 overflow-y-auto py-1">
        {attachedMounts.length > 0 && (
          <section className="mb-2">
            <div className="text-[11px] font-medium text-muted-foreground/80 px-3 pt-2 pb-1">
              附加目录（Agent 可以读取并操作此外部文件夹）
            </div>
            {attachedMounts.map((m) => (
              <AttachedDirRow
                key={m.id}
                mount={m}
                sessionId={sessionId}
                onFileClick={(mt, n, e) => onFileClick?.(mt, n, e)}
              />
            ))}
          </section>
        )}
        {workspaceMount && (
          <WorkspaceFilesBody
            mount={workspaceMount}
            sessionId={sessionId}
            showSubtitle={attachedMounts.length > 0}
            onFileClick={onFileClick}
          />
        )}
      </div>
      <WorkspacePanelFooter workspaceId={currentWorkspaceId ?? null} />
      <MoveToDialog
        workspaceRootPath={workspaceRootPath}
        mountKindForTarget="workspace"
      />
      <DeleteConfirmDialog mountKindForTarget="workspace" />
    </div>
  )
}

function WorkspaceFilesBody({
  mount,
  sessionId,
  showSubtitle,
  onFileClick,
}: {
  mount: MountRoot
  sessionId: string | null
  showSubtitle: boolean
  onFileClick?: (m: MountRoot, n: TreeNode, e: React.MouseEvent<HTMLButtonElement>) => void
}): React.ReactElement {
  const treeApi = useFileTree(mount.id, sessionId)
  useFilesRailWatcher(mount.id, treeApi.applyExternalChanges)

  React.useEffect(() => {
    void filesRailWatchStart(mount.id, sessionId).catch(() => { /* silent */ })
    return () => {
      void filesRailWatchStop(mount.id).catch(() => { /* idempotent */ })
    }
  }, [mount.id, sessionId])

  const siblings = React.useMemo(
    () => new Set(treeApi.nodes.map((n) => n.name)),
    [treeApi.nodes],
  )

  const handleFileClick = React.useCallback(
    (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) =>
      onFileClick?.(mount, node, event),
    [mount, onFileClick],
  )

  return (
    <section>
      {showSubtitle && (
        <div className="text-[11px] font-medium text-muted-foreground/80 px-3 pt-2 pb-1">
          工作文件（存储于该工作区目录）
        </div>
      )}
      {treeApi.loadState === 'error' && (
        <div className="px-3 py-2 text-[11px] text-destructive truncate">
          {treeApi.errorMessage ?? '加载失败'}
        </div>
      )}
      {treeApi.loadState === 'ready' && treeApi.nodes.length === 0 && !showSubtitle && (
        <div className="px-3 py-3 text-[12px] text-muted-foreground">
          工作区还没有文件 — 用下方的「添加文件」或「附加文件夹」开始
        </div>
      )}
      {treeApi.nodes.length > 0 && (
        <div className="min-h-0">
          {treeApi.nodes.map((node) => (
            <FileTreeNode
              key={node.relPath}
              node={node}
              depth={0}
              isExpanded={treeApi.isExpanded}
              onToggle={treeApi.toggleExpand}
              onFileClick={handleFileClick}
              mount={mount}
              sessionId={sessionId}
              siblings={siblings}
            />
          ))}
        </div>
      )}
    </section>
  )
}
```

- [ ] **Step 4: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -10
# Expect: clean. WorkspaceFilesPanel no longer imports MountSection; the file
# still exists but nothing references it after this commit (Task 12 deletes it).
```

- [ ] **Step 5: Run UI tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -5
# Expect: 345 passing — the panel rewrite doesn't add tests, and no existing
# test mounts WorkspaceFilesPanel directly (verified via grep before this task).
```

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/files-rail/hooks/useFileTree.ts ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx
git commit -m "feat(files-rail): WorkspaceFilesPanel new composition + refresh wiring

Panel now composes WorkspacePanelHeader + AttachedDirsSection +
WorkspaceFilesBody + WorkspacePanelFooter; MoveToDialog +
DeleteConfirmDialog mount at panel scope so atom transitions surface
the UI from any row.

useFileTree subscribes to filesRailRefreshTickAtom so the header ↻
button reloads every visible mount in one click.

MountSection.tsx is now an orphan (no live imports); Task 12 deletes it."

git log --oneline -1
```

---

## Task 12: Delete `MountSection.tsx` + clean up `SidePanel.tsx` + tighten `FileTreeNode` props

**Files:**
- Delete: `ui/src/components/files-rail/workspace/MountSection.tsx`
- Modify: `ui/src/components/agent/SidePanel.tsx` (drop the legacy "附加目录" section + attach buttons; keep the FilesRail wiring)
- Modify: `ui/src/components/files-rail/workspace/FileTreeNode.tsx` (make `mount` + `sessionId` + `siblings` props required again — the only remaining callers (AttachedDirRow + WorkspaceFilesBody) always pass them)

- [ ] **Step 1: Branch hygiene + delete `MountSection.tsx`**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
git rm ui/src/components/files-rail/workspace/MountSection.tsx
```

- [ ] **Step 2: Strip the old `附加目录` block from `SidePanel.tsx`**

Open `ui/src/components/agent/SidePanel.tsx` and remove the old "附加目录" UI section (currently around lines 178–221 — the block bounded by the `===== Attached directories section =====` comment). The handlers (`handleAttachWorkspaceDir`, `handleDetachWorkspaceDir`, `handleAttachSessionDir`, `handleDetachSessionDir`) **must be removed too** — the footer in `WorkspacePanelFooter` covers workspace-level attach via the same IPC; session-level attach is out of scope per the spec.

Verify the resulting structure matches:

```tsx
return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* ===== Files Rail (W3) ===== */}
          <div className="flex-1 min-h-0 flex flex-col">
            <FilesRail
              sessionId={sessionId}
              onFileClick={/* unchanged */}
            />
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
          请选择工作区
        </div>
      )}
    </div>
  )
```

Drop the now-unused imports in `SidePanel.tsx`:
- Remove `Plus`, `X`, `FolderPlus` from the `lucide-react` import.
- Remove `attachWorkspaceDirectory`, `detachWorkspaceDirectory`, `attachSessionDirectory`, `detachSessionDirectory`, `openFolderDialog` from the `@/lib/tauri-bridge` import.
- Remove `wsAttachedMap`, `setWsAttachedMap`, `sessionAttachedMap`, `setSessionAttachedMap`, `wsAttachedDirs`, `sessionAttachedDirs`, `allAttachedDirs` — these were only used by the deleted section.
- Remove `agentSessionAttachedDirsMapAtom` and `workspaceAttachedDirsMapAtom` from the `@/atoms/agent-atoms` import.

After editing, run typecheck.

- [ ] **Step 3: Tighten `FileTreeNode` props to required**

Open `ui/src/components/files-rail/workspace/FileTreeNode.tsx`. Find the `FileTreeNodeProps` interface and flip `mount`, `sessionId`, `siblings` from optional to required:

Before:
```tsx
interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
  /** Mount this node belongs to ... Optional for backward-compat ... */
  mount?: MountRoot
  /** Active session ID ... Optional, see above. */
  sessionId?: string | null
  /** Sibling names at this depth ... */
  siblings?: Set<string>
  /** Called after a successful rename ... */
  onRenamed?: (info: { mountId: string; oldRelPath: string; newRelPath: string }) => void
}
```

After:
```tsx
interface FileTreeNodeProps {
  node: TreeNode
  depth: number
  isExpanded: (rel: string) => boolean
  onToggle: (rel: string, isDir: boolean) => Promise<void>
  onFileClick: (node: TreeNode, event: React.MouseEvent<HTMLButtonElement>) => void
  /** Mount this node belongs to — drives menu gating and IPC routing. */
  mount: MountRoot
  /** Active session ID (for addPendingAttachmentAction). */
  sessionId: string | null
  /** Sibling names at this depth — used by RenameInput for dup detection. */
  siblings: Set<string>
  /** Called after a successful rename so the panel can refetch. */
  onRenamed?: (info: { mountId: string; oldRelPath: string; newRelPath: string }) => void
}
```

Also simplify the component body: the `hasMenuContext` guard is no longer needed.

Find:
```tsx
  // Legacy MountSection callers don't pass `mount` — skip the menu/rename code
  // path entirely so behaviour matches the pre-Task-9 file.
  const hasMenuContext = mount !== undefined
  const absolutePath = mount ? `${mount.path}/${node.relPath}` : ''
```

Replace with:
```tsx
  const absolutePath = `${mount.path}/${node.relPath}`
```

Find:
```tsx
  const isRenaming = hasMenuContext && renamingPath === absolutePath
```

Replace with:
```tsx
  const isRenaming = renamingPath === absolutePath
```

Find the rename-commit guard:
```tsx
  const handleRenameCommit = React.useCallback(async (newName: string) => {
    if (!mount) return
    if (newName === node.name) {
```

Replace with:
```tsx
  const handleRenameCommit = React.useCallback(async (newName: string) => {
    if (newName === node.name) {
```

Find the JSX guards:
```tsx
          {isRenaming && mount && siblings ? (
```

Replace with:
```tsx
          {isRenaming ? (
```

Find:
```tsx
        {hasMenuContext && mount && !isRenaming && (
          <FileRowMenu
            mount={mount}
            sessionId={sessionId ?? null}
```

Replace with:
```tsx
        {!isRenaming && (
          <FileRowMenu
            mount={mount}
            sessionId={sessionId}
```

- [ ] **Step 4: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -20
# Expect: clean. If anything still fails, it points to a missed import / handler.
```

- [ ] **Step 5: Run full UI test suite**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -10
# Expect: 51 test files. Tests count should be 328 (baseline) + 8 (helpers) + 6 (RenameInput) + 3 (FileRowMenu) = 345.
# If anything fails outside our new tests, investigate.
```

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add -A
git commit -m "feat(files-rail): delete MountSection + tighten FileTreeNode props

MountSection is superseded by AttachedDirRow + WorkspaceFilesBody.
SidePanel's old 附加目录 UI section (Plus/X buttons + scope-tagged
rows) moves entirely into the new files-rail panel. Session-level
attach was a power-user feature not surfaced in Proma's layout —
the underlying IPC remains, only the UI affordance is dropped.

FileTreeNode props (mount / sessionId / siblings) flip back from
optional to required now that all callers pass them; this catches
future regressions at compile time."

git log --oneline -1
```

---

## Task 13: Section 8a — chat composer chip icon parity

**Files:**
- Modify: `ui/src/components/chat/AttachmentPreviewItem.tsx`

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Replace the ext-badge with `<FileTypeIcon>`**

Open `ui/src/components/chat/AttachmentPreviewItem.tsx`.

Find the import block:

```tsx
import * as React from 'react'
import { X, FileText } from 'lucide-react'
import { cn } from '@/lib/utils'
import { ImageLightbox } from '@/components/ui/image-lightbox'
```

Replace with:

```tsx
import * as React from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { ImageLightbox } from '@/components/ui/image-lightbox'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
```

Find this helper (lines 26-30):

```tsx
function fileExtBadge(filename: string): string {
  const dot = filename.lastIndexOf('.')
  if (dot < 0 || dot === filename.length - 1) return ''
  return filename.slice(dot + 1).toUpperCase().slice(0, 4)
}
```

Delete it entirely (the function becomes dead code).

Find this block (around line 78-98 — the non-image branch JSX):

```tsx
  const ext = fileExtBadge(filename)
  return (
    <div
      className={cn(
        'group/attachment relative flex items-center gap-1.5 shrink-0',
        'rounded-md bg-foreground/[0.04] border border-border/60',
        'pl-1.5 pr-6 py-1 text-[12px] text-foreground/85',
        'transition-colors hover:bg-foreground/[0.07] hover:border-border',
        className
      )}
      title={filename}
    >
      <span
        className={cn(
          'inline-flex items-center justify-center shrink-0',
          'h-[16px] min-w-[22px] px-1 rounded-sm',
          'bg-primary/12 text-primary text-[9.5px] font-semibold tracking-wide tabular-nums',
        )}
      >
        {ext || <FileText className="size-3" />}
      </span>
```

Replace with:

```tsx
  return (
    <div
      className={cn(
        'group/attachment relative flex items-center gap-1.5 shrink-0',
        'rounded-md bg-foreground/[0.04] border border-border/60',
        'pl-1.5 pr-6 py-1 text-[12px] text-foreground/85',
        'transition-colors hover:bg-foreground/[0.07] hover:border-border',
        className
      )}
      title={filename}
    >
      <FileTypeIcon name={filename} isDirectory={false} size={14} className="shrink-0" />
```

(The rest of the JSX — filename span + remove button + closing `</div>` — stays unchanged.)

- [ ] **Step 3: Typecheck**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: clean
```

- [ ] **Step 4: Run UI tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -5
# Expect: all green; total stays ≈ 345.
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment

git add ui/src/components/chat/AttachmentPreviewItem.tsx
git commit -m "feat(chat): use FileTypeIcon in attached-file chips (spec §8a)

Replace the hand-rolled uppercase ext-badge (JS/MD/PNG/...) with the
same @react-symbols/icons glyph the files rail uses. A file dragged
from the rail to chat now keeps an identical icon end-to-end. Drops
the now-unused fileExtBadge helper and FileText import."

git log --oneline -1
```

---

## Task 14: Final verification + push

**Files:** none modified — verification only.

- [ ] **Step 1: Branch hygiene**

```bash
cd /Users/ryanliu/Documents/uclaw && git branch --show-current   # claude/files-rail-proma-alignment
```

- [ ] **Step 2: Frontend gate**

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npx tsc --noEmit 2>&1 | head -5
# Expect: no output
```

```bash
npm test -- --run 2>&1 | tail -10
# Expect:
#   Test Files  51 passed (51)
#        Tests  ~345 passed   (328 baseline + 8 helpers + 6 RenameInput + 3 FileRowMenu = 345)
```

- [ ] **Step 3: Backend sanity (no Rust changes were made; just ensure nothing else regressed)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri
cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
cargo test --lib 2>&1 | tail -5
# Expect: 496 passed
```

- [ ] **Step 4: Manual smoke checklist (pre-push)**

Run `cargo tauri dev` and verify against the Proma reference screenshot:

- [ ] Header row shows `工作区文件` + ⓘ tooltip + ↻ refresh + ↗ Finder.
- [ ] Click ↗ opens the workspace dir in Finder.
- [ ] Click ↻ refetches every visible mount (spinning animation while loading).
- [ ] `附加目录` subtitle + collapsible rows render when at least one attached dir exists.
- [ ] Each attached row toggles open / closed; Lock badge appears on read-only mounts with hover tooltip.
- [ ] `工作文件` subtitle appears only when 附加目录 has rows above.
- [ ] Empty state copy shows `工作区还没有文件 — 用下方的「添加文件」或「附加文件夹」开始` when no attached dirs + no workspace files.
- [ ] Footer 添加文件 opens the native file picker; on pick, file is copied to workspace.
- [ ] Footer 附加文件夹 opens the native folder picker; on pick, mount appears in 附加目录.
- [ ] Hover a workspace file → 3-dot button appears; click → menu lists all 5 items enabled.
- [ ] Hover an attached-dir file → menu shows 5 items but 移动到… / 重命名 / 删除 disabled with tooltip `只读 — 编辑此挂载点需要批准`.
- [ ] 添加到聊天 from a file row produces a chip in the chat input with the same icon as the row.
- [ ] 重命名 a workspace file with the same name → no-op (commits as-is). Duplicate sibling shows inline `已存在同名文件`. Separator chars show `名称不能包含 / \ :`.
- [ ] 移动到… opens OS folder picker; picking outside the workspace toasts `只能移动到当前工作区内的文件夹`; valid pick succeeds and the file disappears from source / appears in destination.
- [ ] 删除 opens AlertDialog showing the file name; confirm deletes; cancel keeps file.

- [ ] **Step 5: Push**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/files-rail-proma-alignment
git log --oneline | head -15
git push -u origin claude/files-rail-proma-alignment 2>&1 | tail -5
```

- [ ] **Step 6: (Optional) open PR — controller decides**

The controller agent / human surfaces a PR after approval. Plan does NOT auto-open. Suggested PR body summary:

```
Files-rail UI alignment with Proma. Layout: header + 附加目录 + 工作文件
+ footer. Per-file 3-dot hover menu (添加到聊天 / 在文件夹中显示 /
移动到… / 重命名 / 删除) with read-only gating on attached dirs.
Chat composer chip uses the same @react-symbols/icons glyph as the
rail (spec § 8a). Zero Rust changes. 14 commits, fully bisectable.
```

---

## Test Floor

Baseline (`main` at `38fb5c5`): **328 UI tests**, **496 Rust tests**.
Target after Task 13: **~345 UI tests** (+8 helpers, +6 RenameInput, +3 FileRowMenu), **496 Rust tests** (unchanged).

## Out-of-Scope (do not implement)

Per spec § Out of Scope:

1. Multi-select / Cmd-click on file rows.
2. Rename / move / delete on session-mount or attached-dir files.
3. Custom in-app folder browser for 移动到…
4. Bulk drag-drop reordering.
5. Per-row refresh icon (one header ↻ covers all mounts).
6. Trash / undo for delete.

If you find yourself wanting any of the above, stop and escalate — they belong in a follow-up plan.

/**
 * SidebarGitActions — the "提交 ▾" affordance after relocation from the
 * chat composer footer to the LeftSidebar capability row (spec
 * `2026-05-13-sidebar-git-actions-relocation-design.md`).
 *
 * Renders `null` when no workspace is active or the workspace has no
 * directory attached, so the row collapses cleanly to just the
 * MCP·Skills indicator. The hairline divider lives inside this file —
 * not in `LeftSidebar.tsx` — so it disappears with the rest of the
 * sidebar git surface when there's nothing to show.
 *
 * Duplicates the `gitIsRepo + gitCurrentBranch` probe from `GitChipsRow`
 * (one extra consumer); see spec §3.1 for the cost rationale. The new
 * `branchSyncTickAtom` keeps composer's `BranchPicker` label in sync
 * when a create-branch flow runs from the sidebar.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { activeWorkspaceCwdAtom, branchSyncTickAtom } from '@/atoms/workspace'
import { gitIsRepo, gitCurrentBranch } from '@/modules/git/api'
import { GitActionsPicker } from '@/components/chat/git/GitActionsPicker'
import { GitWorkbenchDialog } from '@/components/chat/git/GitWorkbenchDialog'

export function SidebarGitActions(): React.ReactElement | null {
  const cwd = useAtomValue(activeWorkspaceCwdAtom)
  const bumpBranchSync = useSetAtom(branchSyncTickAtom)
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
  }, [cwd])

  if (!cwd) return null

  return (
    <>
      {/* Hairline divider between MCP·Skills and 提交 sections. Lives
          inside this file so it vanishes together with the picker when
          cwd is missing — no orphan vertical line. */}
      <div
        className="self-center w-px h-4 bg-foreground/10"
        aria-hidden="true"
      />
      <GitActionsPicker
        variant="sidebar"
        cwd={cwd}
        isGitRepo={isGitRepo}
        onGitRepoChanged={() => {
          // Re-flip local state so the trigger swaps from amber init
          // chip to the default 提交 chip without waiting for the cwd
          // effect to re-fire.
          setIsGitRepo(true)
        }}
        onBranchChange={(branch) => {
          setCurrentBranch(branch)
          // Notify the composer's BranchPicker (which caches the
          // branch label in its own React state) to re-probe. See
          // workspace.ts §branchSyncTickAtom for the rationale.
          bumpBranchSync((t) => t + 1)
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

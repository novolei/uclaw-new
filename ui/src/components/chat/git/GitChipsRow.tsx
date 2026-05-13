/**
 * GitChipsRow — composer footer wrapper for the `BranchPicker`.
 *
 * As of the 2026-05-13 sidebar-git-actions relocation, this row no
 * longer hosts `GitActionsPicker` or `GitWorkbenchDialog` — those now
 * live in `LeftSidebar` via `<SidebarGitActions />`. What remains here
 * is per-conversation routing context (the branch a message would be
 * "on the side of"), which legitimately belongs adjacent to the
 * composer.
 *
 * Reads `activeWorkspaceCwdAtom` directly; renders nothing when no
 * workspace has a directory attached. Per CLAUDE.md dual-composer rule
 * this stays imported in BOTH `ChatInput.tsx` AND `AgentView.tsx`.
 *
 * The `branchSyncTickAtom` is included in the probe `useEffect` deps so
 * that a create-branch flow run from `SidebarGitActions` (which lives
 * on the other side of the layout from this row) re-probes
 * `gitCurrentBranch(cwd)` and updates the label without the user having
 * to reopen the branch picker.
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
    // branchSyncTick is included so a sidebar-initiated branch creation
    // forces this row to re-fetch the current branch and refresh its
    // label without the user having to open the picker. cwd change
    // alone doesn't cover that case because the cwd is unchanged.
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

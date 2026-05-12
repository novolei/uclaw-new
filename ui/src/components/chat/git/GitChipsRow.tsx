/**
 * GitChipsRow — composer footer container for the 3 git affordances.
 *
 * Reads activeWorkspaceCwdAtom directly; renders nothing when no
 * workspace has a directory attached (W6 spec §12.3). Composes the
 * BranchPicker + GitActionsPicker + GitWorkbenchDialog into a single
 * unit so each composer (ChatInput, AgentView) only imports one thing.
 *
 * Per CLAUDE.md dual-composer rule: imported in BOTH ChatInput.tsx
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
    setIsGitRepo(null)  // probing state
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

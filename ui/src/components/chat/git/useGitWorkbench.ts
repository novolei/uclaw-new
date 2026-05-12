/**
 * useGitWorkbench — state + reload for the GitWorkbenchDialog.
 *
 * Extracted from if2Ai's GitWorkbenchDialog.tsx (lines 39-150) into a
 * custom hook because the monolithic component (419 LOC) exceeds uClaw's
 * 400 hard cap. Pure logic — no JSX. The component file
 * (`GitWorkbenchDialog.tsx`) renders the Dialog JSX consuming these outputs.
 *
 * Verbatim port: every state, every reload branch, every side-effect
 * matches if2Ai. Three sub-states (statusState/diffState/branchesState)
 * each follow the 5-variant ViewState<T> discriminated union.
 */

import * as React from 'react'
import {
  gitBranches,
  gitDiff,
  gitStatus,
  parseBranchList,
  type BranchListItem,
} from '@/modules/git/api'

export type Tab = 'status' | 'diff' | 'branches'

/** Generic four-state view-model for a single async fetch. */
export type ViewState<T> =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'empty' }
  | { kind: 'ready'; data: T }
  | { kind: 'error'; message: string }

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

export function useGitWorkbench({ open, cwd }: UseGitWorkbenchArgs): UseGitWorkbenchResult {
  const [tab, setTab] = React.useState<Tab>('status')
  const [statusState, setStatusState] = React.useState<ViewState<string>>({ kind: 'idle' })
  const [diffState, setDiffState] = React.useState<ViewState<string>>({ kind: 'idle' })
  const [branchesState, setBranchesState] = React.useState<ViewState<BranchListItem[]>>({
    kind: 'idle',
  })
  // Diff defaults to `--stat`; user toggles "完整 patch" to switch to full.
  // toggle state changes trigger reload(diff) in the effect below.
  const [diffFull, setDiffFull] = React.useState(false)

  const reload = React.useCallback(
    async (which: Tab | 'all') => {
      if (!cwd) return
      const targets = which === 'all' ? (['status', 'diff', 'branches'] as const) : ([which] as const)
      await Promise.all(
        targets.map(async (t) => {
          if (t === 'status') {
            setStatusState({ kind: 'loading' })
            try {
              const text = await gitStatus(cwd)
              setStatusState(text === null ? { kind: 'empty' } : { kind: 'ready', data: text })
            } catch (err) {
              setStatusState({
                kind: 'error',
                message: err instanceof Error ? err.message : String(err),
              })
            }
          } else if (t === 'diff') {
            setDiffState({ kind: 'loading' })
            try {
              const text = await gitDiff(cwd, { full: diffFull })
              setDiffState(text === null ? { kind: 'empty' } : { kind: 'ready', data: text })
            } catch (err) {
              setDiffState({
                kind: 'error',
                message: err instanceof Error ? err.message : String(err),
              })
            }
          } else {
            setBranchesState({ kind: 'loading' })
            try {
              const raw = await gitBranches(cwd)
              const list = parseBranchList(raw)
              setBranchesState(
                list.length === 0 ? { kind: 'empty' } : { kind: 'ready', data: list },
              )
            } catch (err) {
              setBranchesState({
                kind: 'error',
                message: err instanceof Error ? err.message : String(err),
              })
            }
          }
        }),
      )
    },
    [cwd, diffFull],
  )

  // Reload diff when stat ⇄ full toggle flips (only when open + tab=diff).
  React.useEffect(() => {
    if (open && tab === 'diff') {
      void reload('diff')
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [diffFull])

  // On dialog open, refresh all three tabs; on close, reset state.
  React.useEffect(() => {
    if (open) {
      void reload('all')
    } else {
      setStatusState({ kind: 'idle' })
      setDiffState({ kind: 'idle' })
      setBranchesState({ kind: 'idle' })
      setTab('status')
    }
  }, [open, reload])

  return {
    tab,
    setTab,
    statusState,
    diffState,
    branchesState,
    diffFull,
    setDiffFull,
    reload,
  }
}

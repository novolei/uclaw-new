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

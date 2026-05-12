import { atom } from 'jotai'
import {
  getMonthCostTotal,
  listWorkspaceCostRollup,
  getSettings,
  patchSettings,
} from '@/lib/tauri-bridge'
import type { WorkspaceCostRollup } from '@/lib/types'

/**
 * Start of the current month at local midnight, in epoch ms.
 * Computed once per atom-read (stable through a session). If the user
 * keeps the app open across the 1st of a new month, this stays on the
 * previous month until next read — acceptable for a usage tracker.
 */
export const monthStartMsAtom = atom<number>(() => {
  const now = new Date()
  return new Date(now.getFullYear(), now.getMonth(), 1).getTime()
})

/** Month-to-date total in USD. `null` until first refresh. */
export const monthTotalUsdAtom = atom<number | null>(null)

/** Per-workspace rollup for the current month, sorted by spend desc. */
export const workspaceRollupAtom = atom<WorkspaceCostRollup[]>([])

/**
 * Which budget thresholds have already fired this session. Resets on
 * app restart — intentional, so a fresh boot in a new month can
 * re-alert if still over budget.
 */
export const firedBudgetThresholdsAtom = atom<Set<80 | 100>>(new Set<80 | 100>())

/** Current monthly budget in USD. `null` means no budget set. */
export const monthlyBudgetUsdAtom = atom<number | null>(null)

/** One-shot loader: fetch from backend and seed the atom. */
export const loadBudgetAtom = atom(null, async (_get, set) => {
  const s = await getSettings()
  set(monthlyBudgetUsdAtom, s.monthlyBudgetUsd ?? null)
})

/** Patch the budget on the backend AND update the atom. */
export const setBudgetAtom = atom(null, async (_get, set, value: number | null) => {
  await patchSettings({ monthlyBudgetUsd: value })
  set(monthlyBudgetUsdAtom, value)
})

/** Refresh both monthly atoms in parallel. */
export const refreshCostsAtom = atom(null, async (get, set) => {
  const since = get(monthStartMsAtom)
  const [total, rollup] = await Promise.all([
    getMonthCostTotal(since),
    listWorkspaceCostRollup(since),
  ])
  set(monthTotalUsdAtom, total)
  set(workspaceRollupAtom, rollup)
})

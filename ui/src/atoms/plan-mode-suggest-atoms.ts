import { atom } from 'jotai'

export interface PlanModeSuggestRequest {
  id: string
  session_id: string
  source: 'keyword' | 'agent'
  matched_pattern?: string
  reason?: string
  preview_steps?: string[]
  fired_at_ms: number
}

/** Keyed by sessionId. Each session has at most one pending suggest at a time. */
export const pendingPlanModeSuggestsAtom = atom<Record<string, PlanModeSuggestRequest | null>>({})

/**
 * Sessions where the user has already acted on a plan-mode-suggest banner
 * in the current UI session. The listener in PlanModeSuggestBanner skips
 * incoming events for sessions in this set, so the banner doesn't re-fire
 * after the user dismisses it. Cleared per session when the user manually
 * changes safety mode (intent has changed → re-evaluate).
 *
 * Not persisted — purely in-memory UI state.
 */
export const silencedPlanModeSessionsAtom = atom<Set<string>>(new Set<string>())

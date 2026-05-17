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

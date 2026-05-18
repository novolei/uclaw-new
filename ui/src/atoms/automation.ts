import { atom } from 'jotai'
import {
  listChatSessionsForSpec,
  type HumaneSpecRow,
  type AutomationActivity,
  type EscalationRow,
  type ChatSessionSummary,
} from '@/lib/tauri-bridge'

export type { HumaneSpecRow, AutomationActivity, EscalationRow, ChatSessionSummary }

export const humaneSpecsAtom = atom<HumaneSpecRow[]>([])
// Alias kept for any legacy imports that still reference automationSpecsAtom
export const automationSpecsAtom = humaneSpecsAtom
export const selectedAutomationIdAtom = atom<string | null>(null)
export const automationActivitiesAtom = atom<Record<string, AutomationActivity[]>>({})
export const pendingEscalationsAtom = atom<EscalationRow[]>([])

/**
 * Phase 2b cluster A — per-spec cache of chat threads.
 *
 * Keyed by spec_id. Populated lazily by `refreshChatSessionsAtom` when
 * the spec-detail page opens its "Chat threads" tab. Components reading
 * a missing key should render the empty-state and call `refresh` once.
 */
export const chatSessionsBySpecAtom = atom<Record<string, ChatSessionSummary[]>>({})

/** Action: re-fetch the chat-session list for a specific spec. */
export const refreshChatSessionsAtom = atom(
  null,
  async (_get, set, specId: string) => {
    const rows = await listChatSessionsForSpec(specId)
    set(chatSessionsBySpecAtom, (prev) => ({ ...prev, [specId]: rows }))
  },
)

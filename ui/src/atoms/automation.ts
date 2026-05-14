import { atom } from 'jotai'
import type { HumaneSpecRow, AutomationActivity, EscalationRow } from '@/lib/tauri-bridge'

export type { HumaneSpecRow, AutomationActivity, EscalationRow }

export const humaneSpecsAtom = atom<HumaneSpecRow[]>([])
// Alias kept for any legacy imports that still reference automationSpecsAtom
export const automationSpecsAtom = humaneSpecsAtom
export const selectedAutomationIdAtom = atom<string | null>(null)
export const automationActivitiesAtom = atom<Record<string, AutomationActivity[]>>({})
export const pendingEscalationsAtom = atom<EscalationRow[]>([])

import { atom } from 'jotai'
import type { AutomationSpecRow, AutomationActivity } from '@/lib/tauri-bridge'

export type { AutomationSpecRow, AutomationActivity }

export const automationSpecsAtom = atom<AutomationSpecRow[]>([])
export const automationPanelOpenAtom = atom(false)
export const selectedAutomationIdAtom = atom<string | null>(null)
export const automationActivitiesAtom = atom<Record<string, AutomationActivity[]>>({})

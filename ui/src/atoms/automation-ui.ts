import { atom } from 'jotai'

export type AutomationTab = 'chat' | 'activity' | 'settings'

// Which spec is selected in SpecList (persists across module switches)
export const automationSelectedSpecIdAtom = atom<string | null>(null)

// Which tab is active in SpecRunSurface
export const automationActiveTabAtom = atom<AutomationTab>('activity')

// D2 sub-view: non-null while viewing a run-session inside the 动态 tab
export const automationActivityRunSessionIdAtom = atom<string | null>(null)

import { atomWithStorage } from 'jotai/utils'
import { atom } from 'jotai'

/** Persisted to localStorage; default off so Dock only shows when user opts in. */
export const bottomDockEnabledAtom = atomWithStorage('dock:enabled', false)

/** Mirrors navigator.onLine + online/offline events. */
export const internetOnlineAtom = atom(true)

/** True when get_app_health Tauri invoke succeeds. */
export const backendOnlineAtom = atom(true)

/**
 * null = not yet polled (initializing)
 * true = memU bridge alive
 * false = bridge offline or not initialized
 */
export const memuOnlineAtom = atom<boolean | null>(null)

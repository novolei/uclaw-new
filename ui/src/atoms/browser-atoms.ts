/**
 * Browser Atoms — AI Browser (Phase 3) state management
 *
 * Manages browser state: running status, tabs, and active tab.
 */

import { atom } from 'jotai'

export interface BrowserTabInfo {
  tabId: string
  url: string
  title: string
}

export interface BrowserState {
  running: boolean
  tabs: BrowserTabInfo[]
  activeTabId: string | null
}

export const browserStateAtom = atom<BrowserState>({
  running: false,
  tabs: [],
  activeTabId: null,
})

export const isBrowserLoadingAtom = atom(false)

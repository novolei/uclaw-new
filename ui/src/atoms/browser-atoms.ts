import { atom } from 'jotai'

// ── Types ─────────────────────────────────────────────────────────────

export interface ScreencastFrameEntry {
  tabId: string
  dataB64: string
  pageWidth: number
  pageHeight: number
  timestamp: number
}

export interface DOMElementEntry {
  index: number
  tag: string
  text: string
  isInViewport: boolean
  boundingBox?: { x: number; y: number; width: number; height: number }
}

export interface BrowserTabEntry {
  tabId: string
  url: string
  title: string
  active: boolean
}

export interface DOMStateEntry {
  url: string
  title: string
  elements: DOMElementEntry[]
  pageText: string
  tabs: BrowserTabEntry[]
  timestamp: number
}

// ── Atoms ─────────────────────────────────────────────────────────────

/** Latest CDP screencast frame per sessionId. */
export const browserScreencastFrameAtom = atom(new Map<string, ScreencastFrameEntry>())

/** Latest DOM state per sessionId (populated on demand by BrowserPanel). */
export const browserDOMStateAtom = atom(new Map<string, DOMStateEntry>())

/** Set of sessionIds that currently have an active screencast subscription. */
export const browserScreencastActiveAtom = atom(new Set<string>())

/** Whether the DOM element bounding-box overlay is visible in BrowserPanel. */
export const browserDOMOverlayVisibleAtom = atom(false)

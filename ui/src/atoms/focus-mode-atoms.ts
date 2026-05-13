/**
 * focus-mode-atoms — state for Focus Mode (hides LeftSidebar +
 * RightSidePanel when a preview is open; reveals them on edge hover).
 *
 *   focusModeAtom           : boolean    — global on/off
 *   focusRevealSideAtom     : 'left' | 'right' | null — which island is shown
 *   focusRevealPinnedAtom   : boolean    — click-inside latch
 *   focusMousePosAtom       : { x, y }   — last mousemove (drives glow opacity + Y trace)
 *
 *   toggleFocusModeAction   — flip; auto-cleans reveal/pin when going OFF
 *   exitFocusModeAction     — force everything to defaults (used by autoExit)
 */

import { atom } from 'jotai'

export const focusModeAtom = atom<boolean>(false)
export const focusRevealSideAtom = atom<'left' | 'right' | null>(null)
export const focusRevealPinnedAtom = atom<boolean>(false)
export const focusMousePosAtom = atom<{ x: number; y: number }>({ x: 0, y: 0 })

export const toggleFocusModeAction = atom(null, (get, set) => {
  const next = !get(focusModeAtom)
  set(focusModeAtom, next)
  if (!next) {
    // Going OFF: scrub transient reveal/pin state so the next ON starts clean.
    set(focusRevealSideAtom, null)
    set(focusRevealPinnedAtom, false)
  }
})

export const exitFocusModeAction = atom(null, (_get, set) => {
  set(focusModeAtom, false)
  set(focusRevealSideAtom, null)
  set(focusRevealPinnedAtom, false)
})

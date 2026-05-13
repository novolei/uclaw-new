/**
 * useWorkspaceSwipe / useWorkspaceArrowSwitch — switch workspaces via
 * (a) macOS Magic Mouse / trackpad horizontal-swipe scoped to a single
 *     element (the LeftSidebar), and
 * (b) Shift+ArrowLeft / Shift+ArrowRight keyboard shortcut window-wide
 *     (Windows / external-keyboard fallback for users without a
 *     touchpad).
 *
 * Both wrap around at the boundaries — going RIGHT past the last
 * workspace lands on the first; going LEFT past the first lands on
 * the last. This matches the dot indicator at the bottom of the
 * LeftSidebar that visualises the workspaces as a ring.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  selectWorkspaceAtom,
  swipeGestureAtom,
} from '@/atoms/workspace'

/** Fraction of the sidebar width the user must drag past to commit. */
const COMMIT_FRACTION = 0.35
/** Visual amplification applied to the wheel-driven offset. Above 1.0
 *  means the same wheel input moves the panel further — gives the
 *  destination card more presence even before the user crosses the
 *  commit threshold, so the transition reads as deliberate and
 *  visible rather than "what just happened?". */
const VISUAL_AMPLIFY = 1.4
/** Fraction of width before rubber-band damping kicks in. Below this the
 *  motion is 1:1 with the wheel — most swipes finish before reaching it. */
const RUBBER_BAND_FROM = 0.65
/** ms of no wheel events that ends a gesture (settle phase begins). */
const GESTURE_END_IDLE_MS = 90
/** ms between two switches (prevents momentum-wheel double-firing). */
const COOLDOWN_MS = 350
/** Horizontal magnitude must dominate vertical by this factor to count. */
const HORIZONTAL_DOMINANCE = 1.6

/**
 * Returns true when the current event target is inside an editable
 * surface — chat input, code editor, etc. Don't hijack arrow scrolling
 * from inside text fields.
 */
function isInsideEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  let node: HTMLElement | null = target
  while (node) {
    const tag = node.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA' || node.isContentEditable) return true
    if (node.getAttribute?.('role') === 'textbox') return true
    node = node.parentElement
  }
  return false
}

/**
 * Apple-style rubber-band damping: returns a softened delta that grows
 * sub-linearly past the natural travel range. `c` controls the
 * stickiness — Apple's UIScrollView uses ~0.55. We tune lower (0.32)
 * because uClaw's swipe is shorter and a stiff rubber band reads as
 * sluggish against the smaller travel.
 */
function rubberBand(distance: number, range: number, c = 0.32): number {
  if (range <= 0) return 0
  const sign = Math.sign(distance)
  const abs = Math.abs(distance)
  // d/(1 + c*d/range) approaches range/c asymptotically as d → ∞.
  return sign * (abs / (1 + (c * abs) / range))
}

/**
 * Listen for horizontal swipe / wheel gestures on `scopeRef.current` only.
 * Drives `swipeGestureAtom` with the live offset (rubber-band damped past
 * the commit threshold) so the LeftSidebar can render BOTH the current
 * and the about-to-arrive workspace, sliding past each other under the
 * finger. On gesture end (idle 130 ms): commits if past threshold,
 * snaps back otherwise.
 */
export function useWorkspaceSwipe(scopeRef: React.RefObject<HTMLElement | null>): void {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const setGesture = useSetAtom(swipeGestureAtom)

  const workspacesRef = React.useRef(workspaces)
  const activeIdRef = React.useRef(activeId)
  workspacesRef.current = workspaces
  activeIdRef.current = activeId

  React.useEffect(() => {
    const el = scopeRef.current
    if (!el) return

    // Raw accumulator: signed wheel delta accumulated during this gesture.
    // Positive = user pushed wheel RIGHT (intent: forward / next workspace).
    let accumRaw = 0
    let cooldownUntil = 0
    let endTimer: ReturnType<typeof setTimeout> | null = null
    let isTracking = false

    /** Called when the gesture has been idle long enough to settle. */
    const settle = (): void => {
      isTracking = false
      const list = workspacesRef.current
      const currIdx = list.findIndex((w) => w.id === activeIdRef.current)
      const width = el.clientWidth
      if (list.length === 0 || currIdx === -1 || width === 0) {
        setGesture(null)
        accumRaw = 0
        return
      }
      const commitDist = width * COMMIT_FRACTION
      const step = accumRaw > 0 ? 1 : -1
      if (Math.abs(accumRaw) >= commitDist) {
        const targetIdx = (currIdx + step + list.length) % list.length
        const target = list[targetIdx]
        if (target && target.id !== activeIdRef.current) {
          // Commit: clearing the gesture lets AnimatePresence's cross-pass
          // animate the rest of the way under control of the variants.
          setGesture(null)
          void selectWorkspace({ id: target.id, direction: step > 0 ? 'forward' : 'backward' })
          cooldownUntil = performance.now() + COOLDOWN_MS
        } else {
          setGesture(null)
        }
      } else {
        // Snap back. Clearing the atom triggers the renderer's spring
        // transition back to translateX: 0.
        setGesture(null)
      }
      accumRaw = 0
    }

    const onWheel = (e: WheelEvent): void => {
      const now = performance.now()
      if (now < cooldownUntil) return

      const dx = e.deltaX
      const dy = e.deltaY

      // Vertical-dominant motion: end any in-flight gesture and bail.
      if (Math.abs(dx) < Math.abs(dy) * HORIZONTAL_DOMINANCE) {
        if (isTracking) {
          if (endTimer) { clearTimeout(endTimer); endTimer = null }
          settle()
        }
        return
      }
      if (isInsideEditable(e.target)) return

      // Stop the browser's built-in horizontal scroll-back navigation.
      e.preventDefault()
      isTracking = true
      accumRaw += dx

      // Compute displayed offset with rubber band past commit distance.
      const list = workspacesRef.current
      const currIdx = list.findIndex((w) => w.id === activeIdRef.current)
      const width = el.clientWidth
      if (currIdx === -1 || width === 0) return

      // Visual offset = -accumRaw (negate so positive wheel = current
      // slides LEFT) × amplification (so the destination card gets more
      // visible space per unit of wheel input). Past `freeRange` of
      // width the motion gets rubber-band damping for the boundary feel.
      const freeRange = width * RUBBER_BAND_FROM
      let displayed = -accumRaw * VISUAL_AMPLIFY
      if (Math.abs(displayed) > freeRange) {
        const sign = Math.sign(displayed)
        const overshoot = Math.abs(displayed) - freeRange
        displayed = sign * (freeRange + rubberBand(overshoot, width - freeRange))
      }

      // Direction & preview workspace.
      const step = accumRaw > 0 ? 1 : -1
      const targetIdx = (currIdx + step + list.length) % list.length
      const previewId = list[targetIdx]?.id ?? null

      setGesture({
        offsetPx: displayed,
        containerWidth: width,
        previewWorkspaceId: previewId,
      })

      // Reset the end timer.
      if (endTimer) clearTimeout(endTimer)
      endTimer = setTimeout(() => {
        endTimer = null
        settle()
      }, GESTURE_END_IDLE_MS)
    }

    el.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      el.removeEventListener('wheel', onWheel)
      if (endTimer) clearTimeout(endTimer)
      setGesture(null)
    }
  }, [scopeRef, selectWorkspace, setGesture])
}

/**
 * Window-level Shift+ArrowLeft / Shift+ArrowRight to cycle workspaces.
 * Wraps around. Skipped when the focused element is editable so it
 * doesn't hijack text-cursor moves inside chat / code editors.
 */
export function useWorkspaceArrowSwitch(): void {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)

  const workspacesRef = React.useRef(workspaces)
  const activeIdRef = React.useRef(activeId)
  workspacesRef.current = workspaces
  activeIdRef.current = activeId

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (!e.shiftKey) return
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return

      // Don't fight text-cursor moves when the user is actually typing.
      // Check the keydown's TARGET — a synthetic-focus check on
      // document.activeElement was too aggressive: editable surfaces
      // (chat input, TipTap) hold focus across workspace switches, so
      // the very first switch caused subsequent presses to bail.
      // The event target is the element with caret focus at the moment
      // of the press — sufficient to distinguish "typing" from "shortcut".
      const target = e.target instanceof Element ? (e.target as HTMLElement) : null
      if (target && isInsideEditable(target)) return

      const list = workspacesRef.current
      const currIdx = list.findIndex((w) => w.id === activeIdRef.current)
      if (currIdx === -1 || list.length === 0) return

      const step = e.key === 'ArrowRight' ? 1 : -1
      const targetIdx = (currIdx + step + list.length) % list.length
      const next = list[targetIdx]
      if (!next || next.id === activeIdRef.current) return

      e.preventDefault()
      void selectWorkspace({ id: next.id, direction: step > 0 ? 'forward' : 'backward' })
    }

    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [selectWorkspace])
}

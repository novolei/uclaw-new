/**
 * useWorkspaceSwipe — macOS Magic Mouse / trackpad horizontal-swipe
 * gesture for switching workspaces, mirroring how macOS swipes between
 * Spaces with a two-finger horizontal trackpad gesture.
 *
 * macOS WebKit emits `wheel` events with `deltaX` for horizontal
 * scroll/swipe gestures. We listen window-level, filter aggressively
 * to avoid triggering on vertical scrolls in chat / file lists, and
 * accumulate horizontal delta until it crosses a threshold — then
 * advance the active workspace by ±1 in the sortOrder. A cooldown
 * prevents a single long swipe from firing multiple switches.
 *
 * Mount once at AppShell level.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  selectWorkspaceAtom,
} from '@/atoms/workspace'

/** Cumulative |deltaX| (in px) that must accumulate before firing. */
const SWIPE_THRESHOLD = 80
/** ms between two switches. Prevents one drag-out swipe from firing twice. */
const COOLDOWN_MS = 700
/** ms of inactivity after which the accumulated delta resets to 0. */
const ACCUMULATOR_RESET_MS = 120
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

export function useWorkspaceSwipe(): void {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)

  // Stable refs so the wheel listener doesn't re-attach on every render.
  const workspacesRef = React.useRef(workspaces)
  const activeIdRef = React.useRef(activeId)
  workspacesRef.current = workspaces
  activeIdRef.current = activeId

  React.useEffect(() => {
    let accumDeltaX = 0
    let lastWheelAt = 0
    let cooldownUntil = 0

    const onWheel = (e: WheelEvent): void => {
      const now = performance.now()
      if (now < cooldownUntil) return

      // Reset accumulator between distinct gestures.
      if (now - lastWheelAt > ACCUMULATOR_RESET_MS) accumDeltaX = 0
      lastWheelAt = now

      const dx = e.deltaX
      const dy = e.deltaY

      // Require horizontal-dominant motion. Vertical scroll inside any
      // pane (chat, file list, code) must not switch workspace.
      if (Math.abs(dx) < Math.abs(dy) * HORIZONTAL_DOMINANCE) return

      // Don't hijack scrolling inside editable text surfaces.
      if (isInsideEditable(e.target)) return

      accumDeltaX += dx

      if (Math.abs(accumDeltaX) < SWIPE_THRESHOLD) return

      // Compute the target workspace.
      const list = workspacesRef.current
      const currIdx = list.findIndex((w) => w.id === activeIdRef.current)
      if (currIdx === -1) {
        accumDeltaX = 0
        return
      }
      // Positive deltaX = swipe RIGHT on the trackpad (content moves left)
      // = "go to the workspace on the right" = next workspace.
      const step = accumDeltaX > 0 ? 1 : -1
      const targetIdx = currIdx + step
      if (targetIdx < 0 || targetIdx >= list.length) {
        // Hit a boundary — reset so a bigger swipe doesn't overshoot
        // multiple cells at once.
        accumDeltaX = 0
        return
      }
      const target = list[targetIdx]
      if (!target) {
        accumDeltaX = 0
        return
      }

      e.preventDefault()
      void selectWorkspace(target.id)
      cooldownUntil = now + COOLDOWN_MS
      accumDeltaX = 0
    }

    // `passive: false` so we can preventDefault (stops the browser's
    // built-in horizontal scroll-back navigation on overshoots).
    window.addEventListener('wheel', onWheel, { passive: false })
    return () => window.removeEventListener('wheel', onWheel)
  }, [selectWorkspace])
}

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

/**
 * Listen for horizontal swipe / wheel gestures on `scopeRef.current` only.
 * If the ref is null at mount time (or never set), the hook is a no-op.
 */
export function useWorkspaceSwipe(scopeRef: React.RefObject<HTMLElement | null>): void {
  const workspaces = useAtomValue(workspacesAtom)
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)

  const workspacesRef = React.useRef(workspaces)
  const activeIdRef = React.useRef(activeId)
  workspacesRef.current = workspaces
  activeIdRef.current = activeId

  React.useEffect(() => {
    const el = scopeRef.current
    if (!el) return

    let accumDeltaX = 0
    let lastWheelAt = 0
    let cooldownUntil = 0

    const onWheel = (e: WheelEvent): void => {
      const now = performance.now()
      if (now < cooldownUntil) return

      if (now - lastWheelAt > ACCUMULATOR_RESET_MS) accumDeltaX = 0
      lastWheelAt = now

      const dx = e.deltaX
      const dy = e.deltaY
      if (Math.abs(dx) < Math.abs(dy) * HORIZONTAL_DOMINANCE) return
      if (isInsideEditable(e.target)) return

      accumDeltaX += dx
      if (Math.abs(accumDeltaX) < SWIPE_THRESHOLD) return

      const list = workspacesRef.current
      const currIdx = list.findIndex((w) => w.id === activeIdRef.current)
      if (currIdx === -1 || list.length === 0) {
        accumDeltaX = 0
        return
      }
      // Positive deltaX = swipe RIGHT on the trackpad → next workspace.
      // Wrap around at boundaries so the workspace ring is endless.
      const step = accumDeltaX > 0 ? 1 : -1
      const targetIdx = (currIdx + step + list.length) % list.length
      const target = list[targetIdx]
      if (!target || target.id === activeIdRef.current) {
        accumDeltaX = 0
        return
      }

      e.preventDefault()
      // Pass gesture direction explicitly so the wrap (last → first)
      // still slides in the gesture direction instead of inverting
      // because of sortOrder comparison.
      void selectWorkspace({ id: target.id, direction: step > 0 ? 'forward' : 'backward' })
      cooldownUntil = now + COOLDOWN_MS
      accumDeltaX = 0
    }

    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [scopeRef, selectWorkspace])
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

      // Don't fight text selection / caret moves inside editable surfaces.
      const target = e.target instanceof Element ? (e.target as HTMLElement) : null
      if (target && isInsideEditable(target)) return
      if (document.activeElement instanceof HTMLElement && isInsideEditable(document.activeElement)) return

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

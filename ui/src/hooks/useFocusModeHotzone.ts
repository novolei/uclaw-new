/**
 * useFocusModeHotzone — drives the focusRevealSideAtom state machine
 * from a single global mousemove listener.
 *
 * Rules:
 *   - When the mouse enters the left/right hot zone (≤ HOT_ZONE_WIDTH px
 *     from edge, y > TOP_EXCLUDE) → reveal that side immediately.
 *   - When the mouse is inside the OPEN island's bounding box (or its
 *     hot zone, treated as a union region) → keep revealed.
 *   - When the mouse leaves that union region → start a 200ms timer →
 *     reveal = null.
 *   - Mouse re-entering the region before the timer fires cancels it.
 *   - When pinned === true → the leave timer is suppressed; pinned is
 *     cleared by FloatingIsland's click-outside handler.
 *   - When Focus Mode is OFF → the listener is not registered at all.
 *
 * Mouse position is mirrored into focusMousePosAtom every move so the
 * glow indicator can read it without subscribing to mousemove twice.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  focusModeAtom,
  focusRevealSideAtom,
  focusRevealPinnedAtom,
  focusMousePosAtom,
} from '@/atoms/focus-mode-atoms'
import {
  isInsideIslandRect,
  HOT_ZONE_WIDTH,
  TOP_EXCLUDE,
} from '@/lib/focus-mode-geometry'

const LEAVE_DELAY_MS = 200

export function useFocusModeHotzone(): void {
  const focusMode = useAtomValue(focusModeAtom)
  const setReveal = useSetAtom(focusRevealSideAtom)
  const setMouse = useSetAtom(focusMousePosAtom)
  // Reads via refs so the listener closure stays stable across renders.
  const revealRef = React.useRef<'left' | 'right' | null>(null)
  const pinnedRef = React.useRef(false)
  const reveal = useAtomValue(focusRevealSideAtom)
  const pinned = useAtomValue(focusRevealPinnedAtom)
  React.useEffect(() => { revealRef.current = reveal }, [reveal])
  React.useEffect(() => { pinnedRef.current = pinned }, [pinned])

  React.useEffect(() => {
    if (!focusMode) return

    let leaveTimer: ReturnType<typeof setTimeout> | null = null
    const clearLeaveTimer = () => {
      if (leaveTimer !== null) {
        clearTimeout(leaveTimer)
        leaveTimer = null
      }
    }

    const onMove = (e: MouseEvent) => {
      // Mirror mouse position for the glow indicator.
      setMouse({ x: e.clientX, y: e.clientY })

      if (pinnedRef.current) return  // pinned freezes the reveal state

      const w = window.innerWidth
      const h = window.innerHeight
      const inLeftZone =
        e.clientX <= HOT_ZONE_WIDTH && e.clientY >= TOP_EXCLUDE
      const inRightZone =
        e.clientX >= w - HOT_ZONE_WIDTH && e.clientY >= TOP_EXCLUDE
      // The island's bounding-box is only relevant when that side is
      // ALREADY visible — it's the "keep open while mouse is inside the
      // open island" guard. Treating an unrevealed island's would-be
      // rect as a trigger zone (the previous behaviour) effectively
      // made the entire 280/400 px strip a hot zone, which is exactly
      // what the user complained about ("浮岛在灯条没出现就弹出来"). The
      // narrow 8 px hot zone is the only thing that should TRIGGER a
      // reveal; the island rect only EXTENDS the keep-open region once
      // the island is on screen.
      const inLeftIsland =
        revealRef.current === 'left' &&
        isInsideIslandRect('left', e.clientX, e.clientY, w, h)
      const inRightIsland =
        revealRef.current === 'right' &&
        isInsideIslandRect('right', e.clientX, e.clientY, w, h)

      const wantLeft = inLeftZone || inLeftIsland
      const wantRight = inRightZone || inRightIsland

      if (wantLeft) {
        clearLeaveTimer()
        if (revealRef.current !== 'left') setReveal('left')
      } else if (wantRight) {
        clearLeaveTimer()
        if (revealRef.current !== 'right') setReveal('right')
      } else if (revealRef.current !== null) {
        // Mouse left the union region — schedule hide.
        if (leaveTimer === null) {
          leaveTimer = setTimeout(() => {
            leaveTimer = null
            setReveal(null)
          }, LEAVE_DELAY_MS)
        }
      }
    }

    window.addEventListener('mousemove', onMove)
    return () => {
      window.removeEventListener('mousemove', onMove)
      clearLeaveTimer()
    }
  }, [focusMode, setReveal, setMouse])
}

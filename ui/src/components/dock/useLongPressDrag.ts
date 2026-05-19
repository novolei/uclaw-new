import * as React from 'react'
import { useDragControls } from 'motion/react'

/**
 * Long-press gate for motion's `Reorder.Item` drag activation.
 *
 * Returns dragControls (pass to `Reorder.Item.dragControls` with
 * `dragListener={false}`) plus pointer handlers that start the drag only
 * after the user holds for `delayMs` without moving more than `tolerancePx`.
 *
 * This preserves the iOS-Springboard feel — tap = open, hold = enter
 * reorder mode — while letting motion's Reorder primitive own the actual
 * drag tracking + layout animations.
 */
export function useLongPressDrag(opts: {
  delayMs?: number
  tolerancePx?: number
} = {}): {
  dragControls: ReturnType<typeof useDragControls>
  pointerHandlers: {
    onPointerDown: React.PointerEventHandler<HTMLElement>
    onPointerMove: React.PointerEventHandler<HTMLElement>
    onPointerUp: React.PointerEventHandler<HTMLElement>
    onPointerCancel: React.PointerEventHandler<HTMLElement>
    onPointerLeave: React.PointerEventHandler<HTMLElement>
  }
} {
  const { delayMs = 200, tolerancePx = 5 } = opts
  const dragControls = useDragControls()
  const timerRef = React.useRef<number | undefined>(undefined)
  const startCoordsRef = React.useRef<{ x: number; y: number } | null>(null)

  const clear = React.useCallback(() => {
    if (timerRef.current !== undefined) {
      window.clearTimeout(timerRef.current)
      timerRef.current = undefined
    }
    startCoordsRef.current = null
  }, [])

  const onPointerDown = React.useCallback<React.PointerEventHandler<HTMLElement>>(
    (event) => {
      // Only respond to primary mouse button + touch + pen. Ignore
      // right-click / middle-click so they keep their own semantics.
      if (event.button !== undefined && event.button !== 0) return
      startCoordsRef.current = { x: event.clientX, y: event.clientY }
      // The event object passed here is captured by closure — motion's
      // `dragControls.start` accepts the original PointerEvent and tracks
      // from there, so even after the delay, the drag origin is correct.
      // React's SyntheticEvents aren't pooled (React 17+), so the event
      // is safe to retain in a closure.
      timerRef.current = window.setTimeout(() => {
        timerRef.current = undefined
        dragControls.start(event)
      }, delayMs)
    },
    [delayMs, dragControls],
  )

  const onPointerMove = React.useCallback<React.PointerEventHandler<HTMLElement>>(
    (event) => {
      // If the pointer wanders too far before the long-press fires, abort —
      // the user is scrolling or canceling the press, not initiating a drag.
      if (timerRef.current === undefined || !startCoordsRef.current) return
      const dx = event.clientX - startCoordsRef.current.x
      const dy = event.clientY - startCoordsRef.current.y
      if (dx * dx + dy * dy > tolerancePx * tolerancePx) clear()
    },
    [clear, tolerancePx],
  )

  // pointerup / cancel / leave all close the gate. Once the gate has
  // already fired (timerRef cleared, drag in progress), these become
  // no-ops — motion's own listeners handle pointerup-during-drag.
  const onPointerUp = clear
  const onPointerCancel = clear
  const onPointerLeave = clear

  // Cleanup on unmount so we don't fire after the item disappears.
  React.useEffect(() => clear, [clear])

  return {
    dragControls,
    pointerHandlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
      onPointerLeave,
    },
  }
}

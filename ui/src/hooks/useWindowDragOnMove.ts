import * as React from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

interface DragStart {
  x: number
  y: number
  pointerId: number
}

interface WindowDragOnMoveOptions {
  threshold?: number
}

export function useWindowDragOnMove(
  options: WindowDragOnMoveOptions = {},
): {
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void
  onPointerMove: (event: React.PointerEvent<HTMLElement>) => void
  onPointerUp: (event: React.PointerEvent<HTMLElement>) => void
  onPointerCancel: (event: React.PointerEvent<HTMLElement>) => void
  consumeClickIfDragged: (event: React.MouseEvent<HTMLElement>) => boolean
} {
  const threshold = options.threshold ?? 5
  const startRef = React.useRef<DragStart | null>(null)
  const draggedRef = React.useRef(false)

  const reset = React.useCallback(() => {
    startRef.current = null
  }, [])

  const onPointerDown = React.useCallback((event: React.PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return
    startRef.current = {
      x: event.clientX,
      y: event.clientY,
      pointerId: event.pointerId,
    }
    draggedRef.current = false
  }, [])

  const onPointerMove = React.useCallback((event: React.PointerEvent<HTMLElement>) => {
    const start = startRef.current
    if (!start || start.pointerId !== event.pointerId) return
    const dx = event.clientX - start.x
    const dy = event.clientY - start.y
    if (Math.hypot(dx, dy) < threshold) return

    draggedRef.current = true
    startRef.current = null
    event.preventDefault()
    getCurrentWindow().startDragging().catch(() => {})
  }, [threshold])

  const consumeClickIfDragged = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    if (!draggedRef.current) return false
    draggedRef.current = false
    event.preventDefault()
    event.stopPropagation()
    return true
  }, [])

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp: reset,
    onPointerCancel: reset,
    consumeClickIfDragged,
  }
}

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MonitorPlay } from 'lucide-react'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserDOMOverlayVisibleAtom,
} from '@/atoms/browser-atoms'
import { BrowserDOMOverlay } from './BrowserDOMOverlay'
import { browserUIMouseEvent, type BrowserUIMouseEventType } from '@/lib/tauri-bridge'

interface BrowserScreencastViewProps {
  sessionId: string
  tabId?: string | null
}

interface CanvasPointMappingInput {
  clientX: number
  clientY: number
  canvasRect: Pick<DOMRect, 'left' | 'top' | 'width' | 'height'>
  pageWidth: number
  pageHeight: number
}

interface CanvasPointerMappingInput extends CanvasPointMappingInput {
  eventType: BrowserUIMouseEventType
}

interface BrowserMouseEventPayload {
  eventType: BrowserUIMouseEventType
  x: number
  y: number
}

export function mapCanvasPointToPagePoint(input: CanvasPointMappingInput): { x: number; y: number } | null {
  const { clientX, clientY, canvasRect, pageWidth, pageHeight } = input
  if (canvasRect.width <= 0 || canvasRect.height <= 0 || pageWidth <= 0 || pageHeight <= 0) {
    return null
  }

  const scale = Math.min(canvasRect.width / pageWidth, canvasRect.height / pageHeight)
  const renderedWidth = pageWidth * scale
  const renderedHeight = pageHeight * scale
  const offsetX = (canvasRect.width - renderedWidth) / 2
  const offsetY = (canvasRect.height - renderedHeight) / 2
  const localX = clientX - canvasRect.left - offsetX
  const localY = clientY - canvasRect.top - offsetY

  if (localX < 0 || localY < 0 || localX > renderedWidth || localY > renderedHeight) {
    return null
  }

  return {
    x: localX / scale,
    y: localY / scale,
  }
}

export function mapCanvasPointerToBrowserMouseEvent(input: CanvasPointerMappingInput): BrowserMouseEventPayload | null {
  const point = mapCanvasPointToPagePoint(input)
  return point ? { eventType: input.eventType, ...point } : null
}

export function BrowserScreencastView({ sessionId, tabId }: BrowserScreencastViewProps): React.ReactElement {
  const frameMap = useAtomValue(browserScreencastFrameAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)

  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const isPointerDownRef = React.useRef(false)
  const mouseEventQueueRef = React.useRef<Promise<void>>(Promise.resolve())
  const [displaySize, setDisplaySize] = React.useState({ w: 0, h: 0 })
  const lastDimsRef = React.useRef({ w: 0, h: 0 })

  const frame = frameMap.get(sessionId)
  const domEntry = domMap.get(sessionId)

  React.useLayoutEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const obs = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect
      setDisplaySize({ w: width, h: height })
    })
    obs.observe(canvas)
    return () => obs.disconnect()
  }, [])

  React.useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !frame) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const binary = atob(frame.dataB64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    const blob = new Blob([bytes], { type: frame.mimeType ?? 'image/jpeg' })

    let cancelled = false
    createImageBitmap(blob).then((bitmap) => {
      if (cancelled) { bitmap.close(); return }
      if (lastDimsRef.current.w !== bitmap.width || lastDimsRef.current.h !== bitmap.height) {
        canvas.width = bitmap.width
        canvas.height = bitmap.height
        lastDimsRef.current = { w: bitmap.width, h: bitmap.height }
      }
      ctx.drawImage(bitmap, 0, 0)
      bitmap.close()
    }).catch(() => {})

    return () => { cancelled = true }
  }, [frame])

  const enqueueMouseEvent = React.useCallback((payload: BrowserMouseEventPayload) => {
    if (!tabId) return
    mouseEventQueueRef.current = mouseEventQueueRef.current
      .catch(() => undefined)
      .then(() => browserUIMouseEvent(sessionId, tabId, payload.eventType, payload.x, payload.y))
      .catch(console.error)
  }, [sessionId, tabId])

  const mapPointerEvent = React.useCallback((
    event: React.PointerEvent<HTMLCanvasElement>,
    eventType: BrowserUIMouseEventType,
  ): BrowserMouseEventPayload | null => {
    const canvas = canvasRef.current
    if (!canvas || !frame || !tabId) return
    return mapCanvasPointerToBrowserMouseEvent({
      eventType,
      clientX: event.clientX,
      clientY: event.clientY,
      canvasRect: canvas.getBoundingClientRect(),
      pageWidth: frame.pageWidth,
      pageHeight: frame.pageHeight,
    })
  }, [frame, tabId])

  const handlePointerDown = React.useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0) return
    const canvas = canvasRef.current
    const payload = mapPointerEvent(event, 'mousePressed')
    if (!payload) return
    event.preventDefault()
    isPointerDownRef.current = true
    canvas?.setPointerCapture?.(event.pointerId)
    enqueueMouseEvent(payload)
  }, [enqueueMouseEvent, mapPointerEvent])

  const handlePointerMove = React.useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isPointerDownRef.current) return
    const payload = mapPointerEvent(event, 'mouseMoved')
    if (!payload) return
    event.preventDefault()
    enqueueMouseEvent(payload)
  }, [enqueueMouseEvent, mapPointerEvent])

  const handlePointerUp = React.useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isPointerDownRef.current) return
    const canvas = canvasRef.current
    const payload = mapPointerEvent(event, 'mouseReleased')
    isPointerDownRef.current = false
    canvas?.releasePointerCapture?.(event.pointerId)
    if (!payload) return
    event.preventDefault()
    enqueueMouseEvent(payload)
  }, [enqueueMouseEvent, mapPointerEvent])

  const handlePointerLeave = React.useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isPointerDownRef.current) return
    const payload = mapPointerEvent(event, 'mouseReleased')
    isPointerDownRef.current = false
    if (payload) enqueueMouseEvent(payload)
  }, [enqueueMouseEvent, mapPointerEvent])

  if (!frame) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground bg-muted/10">
        <MonitorPlay size={36} className="opacity-20 mb-2" />
        <span className="text-sm opacity-40">等待浏览器画面...</span>
      </div>
    )
  }

  return (
    <div className="flex-1 relative overflow-hidden bg-black">
      <canvas
        ref={canvasRef}
        className="w-full h-full object-contain"
        style={{ display: 'block', cursor: tabId ? 'default' : 'not-allowed', touchAction: 'none' }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerLeave}
        onPointerLeave={handlePointerLeave}
      />
      {overlayVisible && domEntry && displaySize.w > 0 && (
        <BrowserDOMOverlay
          elements={domEntry.elements}
          pageWidth={frame.pageWidth}
          pageHeight={frame.pageHeight}
          displayWidth={displaySize.w}
          displayHeight={displaySize.h}
        />
      )}
    </div>
  )
}

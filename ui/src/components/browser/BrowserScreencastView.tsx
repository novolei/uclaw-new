import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MonitorPlay } from 'lucide-react'
import {
  browserScreencastFrameAtom,
  browserDOMStateAtom,
  browserDOMOverlayVisibleAtom,
} from '@/atoms/browser-atoms'
import { BrowserDOMOverlay } from './BrowserDOMOverlay'

interface BrowserScreencastViewProps {
  sessionId: string
}

export function BrowserScreencastView({ sessionId }: BrowserScreencastViewProps): React.ReactElement {
  const frameMap = useAtomValue(browserScreencastFrameAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)

  const canvasRef = React.useRef<HTMLCanvasElement>(null)
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
        style={{ display: 'block' }}
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

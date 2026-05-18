import * as React from 'react'
import { useAtomValue } from 'jotai'
import { MonitorPlay } from 'lucide-react'
import { browserScreencastFrameAtom, browserDOMStateAtom, browserDOMOverlayVisibleAtom } from '@/atoms/browser-atoms'
import { BrowserDOMOverlay } from './BrowserDOMOverlay'

interface BrowserScreencastViewProps {
  sessionId: string
}

export function BrowserScreencastView({ sessionId }: BrowserScreencastViewProps): React.ReactElement {
  const frameMap = useAtomValue(browserScreencastFrameAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const overlayVisible = useAtomValue(browserDOMOverlayVisibleAtom)
  const imgRef = React.useRef<HTMLImageElement>(null)
  const [displaySize, setDisplaySize] = React.useState({ w: 0, h: 0 })

  const frame = frameMap.get(sessionId)
  const domEntry = domMap.get(sessionId)

  React.useLayoutEffect(() => {
    if (!imgRef.current) return
    const obs = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect
      setDisplaySize({ w: width, h: height })
    })
    obs.observe(imgRef.current)
    return () => obs.disconnect()
  }, [])

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
      <img
        ref={imgRef}
        src={`data:image/jpeg;base64,${frame.dataB64}`}
        alt="浏览器实时画面"
        className="w-full h-full object-contain"
        draggable={false}
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

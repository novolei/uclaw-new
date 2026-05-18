import * as React from 'react'
import type { DOMElementEntry } from '@/atoms/browser-atoms'

interface BrowserDOMOverlayProps {
  elements: DOMElementEntry[]
  pageWidth: number
  pageHeight: number
  displayWidth: number
  displayHeight: number
}

export function BrowserDOMOverlay({
  elements,
  pageWidth,
  pageHeight,
  displayWidth,
  displayHeight,
}: BrowserDOMOverlayProps): React.ReactElement {
  const scaleX = displayWidth / (pageWidth || displayWidth)
  const scaleY = displayHeight / (pageHeight || displayHeight)

  const viewable = elements.filter((el) => el.isInViewport && el.boundingBox)

  return (
    <svg
      className="absolute inset-0 pointer-events-none"
      width={displayWidth}
      height={displayHeight}
      viewBox={`0 0 ${displayWidth} ${displayHeight}`}
    >
      {viewable.map((el) => {
        const bb = el.boundingBox!
        const x = bb.x * scaleX
        const y = bb.y * scaleY
        const w = bb.width * scaleX
        const h = bb.height * scaleY
        return (
          <g key={el.index}>
            <rect x={x} y={y} width={w} height={h}
              fill="rgba(59,130,246,0.08)" stroke="rgba(59,130,246,0.7)"
              strokeWidth={1} rx={2} />
            <rect x={x} y={y - 14} width={24} height={14} fill="rgba(59,130,246,0.85)" rx={2} />
            <text x={x + 12} y={y - 4} textAnchor="middle" fontSize={9} fill="white" fontFamily="monospace">
              {el.index}
            </text>
          </g>
        )
      })}
    </svg>
  )
}

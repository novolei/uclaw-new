import { describe, expect, it } from 'vitest'
import { mapCanvasPointToPagePoint } from './BrowserScreencastView'

describe('mapCanvasPointToPagePoint', () => {
  it('maps a contained canvas click to page viewport coordinates', () => {
    const point = mapCanvasPointToPagePoint({
      clientX: 350,
      clientY: 200,
      canvasRect: { left: 100, top: 50, width: 500, height: 300 },
      pageWidth: 1000,
      pageHeight: 500,
    })

    expect(point).toEqual({ x: 500, y: 250 })
  })

  it('ignores clicks in object-contain letterbox space', () => {
    const point = mapCanvasPointToPagePoint({
      clientX: 350,
      clientY: 60,
      canvasRect: { left: 100, top: 50, width: 500, height: 300 },
      pageWidth: 1000,
      pageHeight: 500,
    })

    expect(point).toBeNull()
  })
})

import { describe, expect, it } from 'vitest'
import { mapCanvasPointerToBrowserMouseEvent, mapCanvasPointToPagePoint } from './BrowserScreencastView'

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

describe('mapCanvasPointerToBrowserMouseEvent', () => {
  it('maps pointer drag events to browser mouse events', () => {
    const event = mapCanvasPointerToBrowserMouseEvent({
      eventType: 'mouseMoved',
      clientX: 350,
      clientY: 200,
      canvasRect: { left: 100, top: 50, width: 500, height: 300 },
      pageWidth: 1000,
      pageHeight: 500,
    })

    expect(event).toEqual({ eventType: 'mouseMoved', x: 500, y: 250 })
  })
})

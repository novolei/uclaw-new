import { Container, Graphics } from 'pixi.js'
import type { createStore } from 'jotai'
import { openZoneAtom, homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { ZONES, type Zone } from '../hit-areas'
import type { Layer } from './background-layer'

type JotaiStore = ReturnType<typeof createStore>

/**
 * 8 zone hit-areas. Interactive zones (modal / navigate) get a hover
 * highlight + click router; state-only zones are pure visual anchors with
 * no pointer handling.
 */
export function createZoneLayer(w: number, h: number, store: JotaiStore): Layer {
  const container = new Container()

  function onClick(zone: Zone) {
    if (zone.kind === 'modal' && zone.target) {
      store.set(openZoneAtom, zone.target as 'music' | 'sticky' | 'diary')
    } else if (zone.kind === 'navigate') {
      // Leave HomeOffice and navigate to the requested panel.
      store.set(homeOfficePanelOpenAtom, false)
      if (zone.target === 'skills') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'skills' }))
      } else if (zone.target === 'history') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'history' }))
      }
    }
  }

  // One Graphics per zone; track its layout box so resize() can re-place it.
  type ZoneNode = { zone: Zone; g: Graphics; box: { x: number; y: number; w: number; h: number } }
  const nodes: ZoneNode[] = []

  function boxFor(zone: Zone, sw: number, sh: number) {
    return {
      x: (zone.center.x - zone.w / 2) * sw,
      y: (zone.center.y - zone.h / 2) * sh,
      w: zone.w * sw,
      h: zone.h * sh,
    }
  }

  function drawIdle(g: Graphics, box: ZoneNode['box']) {
    g.clear()
    // Invisible hit area — fully transparent fill keeps the rect interactive.
    g.rect(0, 0, box.w, box.h).fill({ color: 0xffffff, alpha: 0 })
  }

  function drawHover(g: Graphics, box: ZoneNode['box']) {
    g.clear()
    g.rect(0, 0, box.w, box.h).stroke({ width: 3, color: 0xffd97a, alpha: 0.9 })
    g.rect(0, 0, box.w, box.h).fill({ color: 0xffd97a, alpha: 0.08 })
  }

  for (const zone of Object.values(ZONES)) {
    const box = boxFor(zone, w, h)
    const g = new Graphics()
    g.position.set(box.x, box.y)
    const node: ZoneNode = { zone, g, box }
    drawIdle(g, box)

    if (zone.kind !== 'state') {
      g.eventMode = 'static'
      g.cursor = 'pointer'
      g.on('pointerover', () => drawHover(g, node.box))
      g.on('pointerout', () => drawIdle(g, node.box))
      g.on('pointertap', () => onClick(zone))
    }

    container.addChild(g)
    nodes.push(node)
  }

  return {
    container,
    resize: (nw, nh) => {
      for (const node of nodes) {
        node.box = boxFor(node.zone, nw, nh)
        node.g.position.set(node.box.x, node.box.y)
        drawIdle(node.g, node.box)
      }
    },
    destroy: () => container.destroy({ children: true }),
  }
}

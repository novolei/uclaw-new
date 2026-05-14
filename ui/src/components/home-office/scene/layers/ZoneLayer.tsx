import { useCallback, useState } from 'react'
import '@pixi/react'
import { useSetAtom } from 'jotai'
import { Graphics } from 'pixi.js'
import { openZoneAtom, homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { ZONES, type Zone } from '../hit-areas'

type Props = { width: number; height: number }

export function ZoneLayer({ width, height }: Props) {
  const setOpenZone = useSetAtom(openZoneAtom)
  const setPanelOpen = useSetAtom(homeOfficePanelOpenAtom)
  const [hover, setHover] = useState<string | null>(null)

  const onClick = useCallback((zone: Zone) => {
    if (zone.kind === 'modal' && zone.target) {
      // 'music' | 'sticky' | 'diary' — narrowed by Zone type
      setOpenZone(zone.target as 'music' | 'sticky' | 'diary')
    } else if (zone.kind === 'navigate') {
      // Leave HomeOffice and navigate to the requested panel.
      // (Skills / history are existing routes; the consumer handles them.)
      setPanelOpen(false)
      if (zone.target === 'skills') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'skills' }))
      } else if (zone.target === 'history') {
        window.dispatchEvent(new CustomEvent('uclaw:navigate', { detail: 'history' }))
      }
    }
  }, [setOpenZone, setPanelOpen])

  return (
    <pixiContainer>
      {Object.values(ZONES).map(zone => {
        const x = (zone.center.x - zone.w / 2) * width
        const y = (zone.center.y - zone.h / 2) * height
        const w = zone.w * width
        const h = zone.h * height
        const isHover = hover === zone.id
        const interactive = zone.kind !== 'state'
        return (
          <pixiGraphics
            key={zone.id}
            eventMode={interactive ? 'static' : 'none'}
            cursor={interactive ? 'pointer' : 'default'}
            x={x}
            y={y}
            onPointerOver={() => interactive && setHover(zone.id)}
            onPointerOut={() => setHover(prev => (prev === zone.id ? null : prev))}
            onPointerTap={() => interactive && onClick(zone)}
            draw={(g: Graphics) => {
              g.clear()
              if (isHover) {
                g.setStrokeStyle({ width: 3, color: 0xffd97a, alpha: 0.9 })
                g.rect(0, 0, w, h)
                g.stroke()
                g.fill({ color: 0xffd97a, alpha: 0.08 })
                g.rect(0, 0, w, h)
                g.fill()
              } else {
                // Invisible hit area
                g.rect(0, 0, w, h)
                g.fill({ color: 0xffffff, alpha: 0 })
              }
            }}
          />
        )
      })}
    </pixiContainer>
  )
}

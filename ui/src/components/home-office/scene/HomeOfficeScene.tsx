import { useEffect, useRef, useState } from 'react'
import { Application, extend } from '@pixi/react'
import { Container, Graphics, Sprite } from 'pixi.js'
import { BackgroundLayer } from './layers/BackgroundLayer'
import { ZoneLayer } from './layers/ZoneLayer'
import { CharacterLayer } from './layers/CharacterLayer'
import { ParticleLayer } from './layers/ParticleLayer'

// v8 of @pixi/react requires registering pixi classes used in JSX
extend({ Container, Graphics, Sprite })

export function HomeOfficeScene() {
  const wrapRef = useRef<HTMLDivElement | null>(null)
  const [size, setSize] = useState({ w: 1280, h: 720 })

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => {
      for (const e of entries) {
        const { width, height } = e.contentRect
        if (width > 0 && height > 0) setSize({ w: width, h: height })
      }
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  return (
    <div ref={wrapRef} className="relative w-full h-full overflow-hidden bg-content-area">
      <Application
        width={size.w}
        height={size.h}
        background={0x88ccee}
        antialias
        resolution={window.devicePixelRatio || 1}
        autoDensity
      >
        <BackgroundLayer width={size.w} height={size.h} />
        <ZoneLayer width={size.w} height={size.h} />
        <CharacterLayer width={size.w} height={size.h} />
        <ParticleLayer width={size.w} height={size.h} />
      </Application>
    </div>
  )
}

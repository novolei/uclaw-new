import { useEffect, useRef } from 'react'
import { useStore } from 'jotai'
import { Application } from 'pixi.js'
import { createBackgroundLayer, type Layer } from './layers/background-layer'
import { createZoneLayer } from './layers/zone-layer'
import { createCharacterLayer } from './layers/character-layer'
import { createParticleLayer } from './layers/particle-layer'

/**
 * Imperative PixiJS scene host.
 *
 * We drive pixi.js v8 directly rather than through @pixi/react: @pixi/react v8
 * pins react-reconciler@0.31 which requires React 19 internals
 * (`ReactSharedInternals.S`) that don't exist in this app's React 18. The
 * Application + display tree are built in a useEffect, layers subscribe to
 * jotai atoms via the store, and everything is torn down on unmount.
 */
export function HomeOfficeScene() {
  const wrapRef = useRef<HTMLDivElement | null>(null)
  const store = useStore()

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return

    let disposed = false
    let app: Application | null = null
    const layers: Layer[] = []

    const init = async () => {
      const pixiApp = new Application()
      await pixiApp.init({
        resizeTo: el,
        background: 0x88ccee,
        antialias: true,
        resolution: window.devicePixelRatio || 1,
        autoDensity: true,
      })

      // Component unmounted while init() was awaiting — bail and clean up.
      if (disposed) {
        pixiApp.destroy(true, { children: true, texture: true })
        return
      }
      app = pixiApp
      el.appendChild(pixiApp.canvas)

      const w = pixiApp.screen.width
      const h = pixiApp.screen.height

      const background = await createBackgroundLayer(w, h)
      if (disposed) {
        background.destroy()
        pixiApp.destroy(true, { children: true, texture: true })
        return
      }

      const zones = createZoneLayer(w, h, store)
      const character = createCharacterLayer(w, h, store)
      const particles = createParticleLayer(w, h)
      layers.push(background, zones, character, particles)

      // z-order: background → zones → character → particles
      pixiApp.stage.addChild(
        background.container,
        zones.container,
        character.container,
        particles.container,
      )

      pixiApp.ticker.add(ticker => {
        for (const layer of layers) layer.tick?.(ticker.deltaMS)
      })

      pixiApp.renderer.on('resize', (rw: number, rh: number) => {
        for (const layer of layers) layer.resize?.(rw, rh)
      })
    }

    void init()

    return () => {
      disposed = true
      for (const layer of layers) layer.destroy()
      layers.length = 0
      if (app) {
        app.destroy(true, { children: true, texture: true })
        app = null
      }
    }
  }, [store])

  return (
    <div
      ref={wrapRef}
      className="relative w-full h-full overflow-hidden bg-content-area"
    />
  )
}

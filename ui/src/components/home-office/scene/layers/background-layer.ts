import { Assets, Container, Sprite, Texture } from 'pixi.js'
import { toast } from 'sonner'

export type Layer = {
  container: Container
  resize?: (w: number, h: number) => void
  tick?: (deltaMS: number) => void
  destroy: () => void
}

/**
 * Static v5 sky-island background, stretched to fill the scene.
 * Loads async; if the asset is missing the layer stays empty and the
 * Application's solid background colour shows through.
 */
export async function createBackgroundLayer(w: number, h: number): Promise<Layer> {
  const container = new Container()
  let sprite: Sprite | null = null

  try {
    const texture = await Assets.load<Texture>('/home-office/scene-sky-v5.png')
    sprite = new Sprite(texture)
    sprite.width = w
    sprite.height = h
    container.addChild(sprite)
  } catch (err) {
    console.warn('HomeOffice: background failed to load', err)
    toast.error('Home Office assets not found, please reinstall')
  }

  return {
    container,
    resize: (nw, nh) => {
      if (sprite) {
        sprite.width = nw
        sprite.height = nh
      }
    },
    destroy: () => container.destroy({ children: true }),
  }
}

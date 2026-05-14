import { useEffect, useState } from 'react'
import '@pixi/react'
import { Assets, Texture } from 'pixi.js'
import { toast } from 'sonner'

type Props = { width: number; height: number }

export function BackgroundLayer({ width, height }: Props) {
  const [texture, setTexture] = useState<Texture | null>(null)

  useEffect(() => {
    let cancelled = false
    Assets.load<Texture>('/home-office/scene-sky-v5.png').then(t => {
      if (!cancelled) setTexture(t)
    }).catch(err => {
      if (!cancelled) {
        console.warn('HomeOffice: background failed to load', err)
        toast.error('Home Office assets not found, please reinstall')
      }
    })
    return () => { cancelled = true }
  }, [])

  if (!texture) return null
  return (
    <pixiSprite
      texture={texture}
      x={0}
      y={0}
      width={width}
      height={height}
    />
  )
}

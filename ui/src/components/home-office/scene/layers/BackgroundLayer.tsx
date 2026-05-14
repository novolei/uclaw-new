import { useEffect, useState } from 'react'
import '@pixi/react'
import { Assets, Texture } from 'pixi.js'

type Props = { width: number; height: number }

export function BackgroundLayer({ width, height }: Props) {
  const [texture, setTexture] = useState<Texture | null>(null)

  useEffect(() => {
    let cancelled = false
    Assets.load<Texture>('/home-office/scene-sky-v5.png').then(t => {
      if (!cancelled) setTexture(t)
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

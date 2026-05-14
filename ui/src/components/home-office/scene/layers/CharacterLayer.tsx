import { useEffect, useRef, useState } from 'react'
import { useAtomValue } from 'jotai'
import { useTick } from '@pixi/react'
import type { Sprite, Texture } from 'pixi.js'
import {
  characterPositionAtom,
  characterDirectionAtom,
  characterMotionAtom,
  homeOfficeStateAtom,
  type Direction,
} from '@/atoms/home-office-atoms'
import { resolveSpriteKey } from '../dir-utils'
import { loadAnimatedWebpCached } from '../sprite-loader'
import { WebpAnimator } from '../animator'

type Props = { width: number; height: number }

const SPRITE_BASE = '/home-office/sprites/lofi-girl'
const SPRITE_W = 160
const SPRITE_H = 160

function spriteUrlForMotion(
  motion: 'walk' | 'pose',
  direction: Direction,
  state: string,
): { url: string; flipX: boolean } {
  if (motion === 'walk') {
    const { key, flipX } = resolveSpriteKey(direction)
    return { url: `${SPRITE_BASE}/${key}.webp`, flipX }
  }
  // Pose maps off state. 'tool_activity' visually shares 'thinking' pose.
  const poseState = state === 'tool_activity' ? 'thinking' : state
  return { url: `${SPRITE_BASE}/pose-${poseState}.webp`, flipX: false }
}

export function CharacterLayer({ width, height }: Props) {
  const pos = useAtomValue(characterPositionAtom)
  const direction = useAtomValue(characterDirectionAtom)
  const motion = useAtomValue(characterMotionAtom)
  const state = useAtomValue(homeOfficeStateAtom)

  const spriteRef = useRef<Sprite | null>(null)
  const animatorRef = useRef<WebpAnimator | null>(null)
  const [currentFrames, setCurrentFrames] = useState<Texture[]>([])
  const [flipX, setFlipX] = useState(false)

  // Resolve + load frames when motion/direction/state changes
  useEffect(() => {
    const { url, flipX: needsFlip } = spriteUrlForMotion(motion, direction, state)
    setFlipX(needsFlip)
    let cancelled = false
    loadAnimatedWebpCached(url).then(frames => {
      if (cancelled) return
      setCurrentFrames(frames)
      if (animatorRef.current) {
        animatorRef.current.swap(frames)
      } else if (spriteRef.current) {
        animatorRef.current = new WebpAnimator(spriteRef.current, frames, 24)
      }
    }).catch(err => {
      console.warn('HomeOffice: sprite load failed', url, err)
    })
    return () => { cancelled = true }
  }, [motion, direction, state])

  useTick(({ deltaMS }) => {
    animatorRef.current?.tick(deltaMS)
  })

  const screenX = pos.x * width
  const screenY = pos.y * height

  return (
    <pixiSprite
      ref={spriteRef}
      texture={currentFrames[0]}
      x={screenX}
      y={screenY}
      width={SPRITE_W}
      height={SPRITE_H}
      anchor={0.5}
      scale={{ x: flipX ? -1 : 1, y: 1 }}
    />
  )
}

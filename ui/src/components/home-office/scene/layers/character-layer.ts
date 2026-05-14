import { Container, Sprite } from 'pixi.js'
import type { createStore } from 'jotai'
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
import type { Layer } from './background-layer'

type JotaiStore = ReturnType<typeof createStore>

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

/**
 * The Lofi-girl character. Reads the four character atoms from the store:
 *   - position → screen placement
 *   - motion + direction + state → which WebP sprite sequence to play
 * The WebpAnimator is driven by the scene ticker via the returned `tick`.
 */
export function createCharacterLayer(w: number, h: number, store: JotaiStore): Layer {
  const container = new Container()
  const sprite = new Sprite()
  sprite.anchor.set(0.5)
  container.addChild(sprite)

  let animator: WebpAnimator | null = null
  // Monotonic token guards against out-of-order async sprite loads.
  let loadToken = 0
  let sceneW = w
  let sceneH = h

  function applyTransform() {
    const pos = store.get(characterPositionAtom)
    sprite.x = pos.x * sceneW
    sprite.y = pos.y * sceneH
  }

  // Size from the actual texture (sprites are 720x720) and apply the
  // horizontal flip via the scale sign.
  function applySizing(flipX: boolean) {
    const tex = sprite.texture
    const sx = SPRITE_W / (tex.width || SPRITE_W)
    const sy = SPRITE_H / (tex.height || SPRITE_H)
    sprite.scale.set(flipX ? -sx : sx, sy)
  }

  async function reloadSprite() {
    const motion = store.get(characterMotionAtom)
    const direction = store.get(characterDirectionAtom)
    const state = store.get(homeOfficeStateAtom)
    const { url, flipX } = spriteUrlForMotion(motion, direction, state)
    const token = ++loadToken
    try {
      const frames = await loadAnimatedWebpCached(url)
      if (token !== loadToken) return // a newer load superseded this one
      if (animator) animator.swap(frames)
      else animator = new WebpAnimator(sprite, frames, 24)
      applySizing(flipX)
    } catch (err) {
      console.warn('HomeOffice: sprite load failed', url, err)
    }
  }

  const unsubPos = store.sub(characterPositionAtom, applyTransform)
  const unsubMotion = store.sub(characterMotionAtom, reloadSprite)
  const unsubDir = store.sub(characterDirectionAtom, reloadSprite)
  const unsubState = store.sub(homeOfficeStateAtom, reloadSprite)

  applyTransform()
  void reloadSprite()

  return {
    container,
    tick: (deltaMS) => animator?.tick(deltaMS),
    resize: (nw, nh) => {
      sceneW = nw
      sceneH = nh
      applyTransform()
    },
    destroy: () => {
      unsubPos()
      unsubMotion()
      unsubDir()
      unsubState()
      container.destroy({ children: true })
    },
  }
}

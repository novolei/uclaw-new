import { describe, it, expect } from 'vitest'
import { WebpAnimator } from './animator'

// Lightweight Sprite stub with a settable .texture
class FakeSprite { texture: unknown = null }
function fakeTextures(n: number) {
  return Array.from({ length: n }, (_, i) => ({ id: `tex-${i}` }) as unknown)
}

describe('WebpAnimator', () => {
  it('starts on frame 0', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    expect(sprite.texture).toBe(frames[0])
  })

  it('advances one frame per ~42ms at 24fps', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(42) // 1000/24 ≈ 41.67
    expect(sprite.texture).toBe(frames[1])
    a.tick(42)
    expect(sprite.texture).toBe(frames[2])
  })

  it('loops back to frame 0', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(3)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(42 * 3) // ~3 frame ticks
    expect(sprite.texture).toBe(frames[0])
  })

  it('swap() resets to frame 0 of new sequence', () => {
    const sprite = new FakeSprite()
    const seqA = fakeTextures(4)
    const seqB = fakeTextures(2)
    const a = new WebpAnimator(sprite as never, seqA as never, 24)
    a.tick(42 * 2)
    a.swap(seqB as never)
    expect(sprite.texture).toBe(seqB[0])
    a.tick(42)
    expect(sprite.texture).toBe(seqB[1])
  })

  it('accumulates sub-frame deltas correctly', () => {
    const sprite = new FakeSprite()
    const frames = fakeTextures(4)
    const a = new WebpAnimator(sprite as never, frames as never, 24)
    a.tick(20)
    expect(sprite.texture).toBe(frames[0])
    a.tick(25) // 20+25=45 → cross threshold
    expect(sprite.texture).toBe(frames[1])
  })
})

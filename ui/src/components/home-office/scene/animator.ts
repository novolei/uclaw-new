import type { Sprite, Texture } from 'pixi.js'

/**
 * Drives a PIXI.Sprite's .texture through a frame sequence at a fixed fps.
 * Caller invokes tick(deltaMs) on every render frame (PIXI.Ticker).
 */
export class WebpAnimator {
  private currentFrame = 0
  private accumulatedMs = 0
  private readonly frameDurationMs: number

  constructor(
    private readonly sprite: Sprite,
    private frames: Texture[],
    fps = 24,
  ) {
    this.frameDurationMs = 1000 / fps
    if (frames.length > 0) this.sprite.texture = frames[0]
  }

  tick(deltaMs: number) {
    if (this.frames.length === 0) return
    this.accumulatedMs += deltaMs
    while (this.accumulatedMs >= this.frameDurationMs) {
      this.accumulatedMs -= this.frameDurationMs
      this.currentFrame = (this.currentFrame + 1) % this.frames.length
      this.sprite.texture = this.frames[this.currentFrame]
    }
  }

  swap(newFrames: Texture[]) {
    this.frames = newFrames
    this.currentFrame = 0
    this.accumulatedMs = 0
    if (newFrames.length > 0) this.sprite.texture = newFrames[0]
  }
}

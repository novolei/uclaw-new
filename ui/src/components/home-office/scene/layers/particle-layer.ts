import { Container, Graphics } from 'pixi.js'
import type { Layer } from './background-layer'

type Petal = { x: number; y: number; vx: number; vy: number; rot: number; r: number }
type Kodama = { cx: number; cy: number; rx: number; ry: number; phase: number; speed: number }
type Mist = { x: number; y: number; life: number; maxLife: number }

const PETAL_COUNT = 8
const KODAMA_COUNT = 3
const MIST_COUNT = 10

function spawnPetal(width: number): Petal {
  return {
    x: width * 0.5 + Math.random() * width * 0.5,
    y: -10,
    vx: -0.06 - Math.random() * 0.04,
    vy: 0.04 + Math.random() * 0.03,
    rot: Math.random() * Math.PI * 2,
    r: 4 + Math.random() * 3,
  }
}

function spawnMist(width: number, height: number): Mist {
  const onLeft = Math.random() < 0.5
  return {
    x: onLeft ? width * 0.3 : width * 0.78,
    y: height * 0.82,
    life: 0,
    maxLife: 800 + Math.random() * 400,
  }
}

/**
 * Ambient particle effects drawn into a single Graphics:
 *   - sakura petals drifting down-and-left
 *   - kodama spirits bobbing along Lissajous paths
 *   - waterfall mist rising + fading near the two waterfalls
 * Driven by the scene ticker via the returned `tick`.
 */
export function createParticleLayer(w: number, h: number): Layer {
  const container = new Container()
  const g = new Graphics()
  container.addChild(g)

  let sceneW = w
  let sceneH = h

  const petals: Petal[] = Array.from({ length: PETAL_COUNT }, () => spawnPetal(sceneW))
  let kodama: Kodama[] = makeKodama(sceneW, sceneH)
  const mist: Mist[] = Array.from({ length: MIST_COUNT }, () => spawnMist(sceneW, sceneH))

  function makeKodama(width: number, height: number): Kodama[] {
    return Array.from({ length: KODAMA_COUNT }, (_, i) => ({
      cx: width * (0.3 + i * 0.18),
      cy: height * (0.55 + (i % 2) * 0.08),
      rx: 40 + Math.random() * 20,
      ry: 20 + Math.random() * 15,
      phase: Math.random() * Math.PI * 2,
      speed: 0.0006 + Math.random() * 0.0004,
    }))
  }

  function tick(deltaMS: number) {
    for (const p of petals) {
      p.x += p.vx * deltaMS
      p.y += p.vy * deltaMS
      p.rot += 0.002 * deltaMS
      if (p.y > sceneH + 10 || p.x < -10) {
        Object.assign(p, spawnPetal(sceneW))
      }
    }

    for (const k of kodama) {
      k.phase += k.speed * deltaMS
    }

    for (const m of mist) {
      m.life += deltaMS
      if (m.life > m.maxLife) Object.assign(m, spawnMist(sceneW, sceneH))
    }

    g.clear()

    for (const p of petals) {
      g.ellipse(p.x, p.y, p.r, p.r * 0.55).fill({ color: 0xffc1d1, alpha: 0.8 })
    }

    for (const k of kodama) {
      const kx = k.cx + Math.cos(k.phase) * k.rx
      const ky = k.cy + Math.sin(k.phase * 1.3) * k.ry
      g.circle(kx, ky, 6).fill({ color: 0xffffff, alpha: 0.9 })
      g.circle(kx - 2, ky - 1, 0.9).fill({ color: 0x222222, alpha: 1 })
      g.circle(kx + 2, ky - 1, 0.9).fill({ color: 0x222222, alpha: 1 })
    }

    for (const m of mist) {
      const t = m.life / m.maxLife
      const yOff = -t * 40
      const alpha = (1 - t) * 0.35
      g.circle(m.x + Math.sin(t * 3) * 4, m.y + yOff, 6 + t * 5).fill({ color: 0xffffff, alpha })
    }
  }

  return {
    container,
    tick,
    resize: (nw, nh) => {
      sceneW = nw
      sceneH = nh
      kodama = makeKodama(sceneW, sceneH)
    },
    destroy: () => container.destroy({ children: true }),
  }
}

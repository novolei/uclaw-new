// ─── Layout types ────────────────────────────────────────────────────────

export interface LayoutNode { id: string; kind: string }
export interface LayoutEdge { from: string; to: string }

export interface NodePosition {
  id: string
  x: number
  y: number
  z: number
}

// ─── Math helpers ────────────────────────────────────────────────────────

export function gaussian(): number {
  const u1 = Math.random() || 0.001
  const u2 = Math.random()
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2)
}

export function hashCode(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0
  }
  return Math.abs(h)
}

// ─── 螺旋星系布局 ────────────────────────────────────────────────────────

/**
 * Compute a spiral-galaxy layout for a set of nodes and edges.
 *
 * Nodes are bucketed into three rings by kind:
 *   core   — boot, identity, directive
 *   mid    — value, user_profile, curated
 *   outer  — everything else (unknown gbrain types land here)
 *
 * `centerOffset` shifts the entire layout, allowing multiple nebulae to be
 * positioned side-by-side in a DualNebulaView without overlapping.
 */
export function computeGalaxyLayout(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  centerOffset: [number, number, number] = [0, 0, 0],
): NodePosition[] {
  if (nodes.length === 0) return []

  const coreKinds = ['boot', 'identity', 'directive']
  const midKinds = ['value', 'user_profile', 'curated']

  const coreNodes: LayoutNode[] = []
  const midNodes: LayoutNode[] = []
  const outerNodes: LayoutNode[] = []

  for (const n of nodes) {
    if (coreKinds.includes(n.kind)) coreNodes.push(n)
    else if (midKinds.includes(n.kind)) midNodes.push(n)
    else outerNodes.push(n)
  }

  const armCount = 3

  function placeGroup(group: LayoutNode[], baseRadius: number, spread: number): { id: string; x: number; y: number; z: number }[] {
    const groupSize = group.length || 1
    return group.map((node, idx) => {
      const arm = idx % armCount
      const armBaseAngle = (arm * 2 * Math.PI) / armCount
      const progress = idx / groupSize
      const spiralAngle = armBaseAngle + progress * Math.PI * 2.0
      const r = baseRadius + spread * Math.sqrt(progress)
      const jitter = spread * 0.25

      const x = r * Math.cos(spiralAngle) + gaussian() * jitter
      const y = (Math.random() - 0.5) * spread * 0.3
      const z = r * Math.sin(spiralAngle) + gaussian() * jitter
      return { id: node.id, x, y, z }
    })
  }

  const allPositions = [
    ...placeGroup(coreNodes, 0, 60),
    ...placeGroup(midNodes, 70, 70),
    ...placeGroup(outerNodes, 140, 80),
  ]

  // 力导向迭代优化
  const idxMap = new Map<string, number>()
  const sim = allPositions.map((p, i) => {
    idxMap.set(p.id, i)
    return { ...p, vx: 0, vy: 0, vz: 0 }
  })

  const iterations = 50
  const repulsion = 2000
  const attraction = 0.003
  const centerForce = 0.005
  const damping = 0.8

  for (let iter = 0; iter < iterations; iter++) {
    for (let i = 0; i < sim.length; i++) {
      for (let j = i + 1; j < sim.length; j++) {
        const dx = sim[i].x - sim[j].x
        const dy = sim[i].y - sim[j].y
        const dz = sim[i].z - sim[j].z
        const distSq = dx * dx + dy * dy + dz * dz + 1
        const force = repulsion / distSq
        const dist = Math.sqrt(distSq)
        const fx = (dx / dist) * force
        const fy = (dy / dist) * force
        const fz = (dz / dist) * force
        sim[i].vx += fx; sim[i].vy += fy; sim[i].vz += fz
        sim[j].vx -= fx; sim[j].vy -= fy; sim[j].vz -= fz
      }
    }

    for (const edge of edges) {
      const si = idxMap.get(edge.from)
      const ti = idxMap.get(edge.to)
      if (si === undefined || ti === undefined) continue
      const dx = sim[ti].x - sim[si].x
      const dy = sim[ti].y - sim[si].y
      const dz = sim[ti].z - sim[si].z
      const fx = dx * attraction
      const fy = dy * attraction
      const fz = dz * attraction
      sim[si].vx += fx; sim[si].vy += fy; sim[si].vz += fz
      sim[ti].vx -= fx; sim[ti].vy -= fy; sim[ti].vz -= fz
    }

    for (const p of sim) {
      p.vx -= p.x * centerForce
      p.vy -= p.y * centerForce
      p.vz -= p.z * centerForce
      p.vx *= damping; p.vy *= damping; p.vz *= damping
      p.x += p.vx; p.y += p.vy; p.z += p.vz
    }
  }

  return sim.map(({ id, x, y, z }) => ({
    id,
    x: x + centerOffset[0],
    y: y + centerOffset[1],
    z: z + centerOffset[2],
  }))
}

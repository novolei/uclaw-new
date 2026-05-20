import { computeGalaxyLayout, type NodePosition } from '../nebula/layout'
import type { MemoryGraphData } from '@/lib/types'
import type { KnowledgeGraph } from '@/lib/gbrain-browse'

export type NebulaLayer = 'memory' | 'knowledge'

export interface UnifiedNode {
  id: string
  layer: NebulaLayer
  kind: string
  title: string
  x: number
  y: number
  z: number
}

export interface UnifiedEdge {
  from: string
  to: string
  layer: NebulaLayer
}

export interface UnifiedScene {
  nodes: UnifiedNode[]
  edges: UnifiedEdge[]
  bridges: UnifiedEdge[] // 占位，V1 恒空（桥留 D）
}

const CLUSTER_GAP = 220

export function buildUnifiedScene(
  memory: MemoryGraphData | null,
  knowledge: KnowledgeGraph | null,
): UnifiedScene {
  const nodes: UnifiedNode[] = []
  const edges: UnifiedEdge[] = []

  // 记忆层：暖簇偏 -X
  if (memory && memory.nodes.length > 0) {
    const layoutNodes = memory.nodes.map((n) => ({ id: n.id, kind: n.kind }))
    const layoutEdges = memory.edges.map((e) => ({ from: e.parentNodeId ?? '', to: e.childNodeId }))
    const pos = computeGalaxyLayout(layoutNodes, layoutEdges, [-CLUSTER_GAP, 0, 0])
    const posMap = new Map<string, NodePosition>(pos.map((p) => [p.id, p]))
    for (const n of memory.nodes) {
      const p = posMap.get(n.id)
      if (!p) continue
      nodes.push({ id: n.id, layer: 'memory', kind: n.kind, title: n.title, x: p.x, y: p.y, z: p.z })
    }
    for (const e of memory.edges) {
      if (e.parentNodeId) edges.push({ from: e.parentNodeId, to: e.childNodeId, layer: 'memory' })
    }
  }

  // 知识层：冷簇偏 +X
  if (knowledge && knowledge.nodes.length > 0) {
    const layoutNodes = knowledge.nodes.map((n) => ({ id: n.slug, kind: n.type }))
    const layoutEdges = knowledge.edges.map((e) => ({ from: e.from_slug, to: e.to_slug }))
    const pos = computeGalaxyLayout(layoutNodes, layoutEdges, [CLUSTER_GAP, 0, 0])
    const posMap = new Map<string, NodePosition>(pos.map((p) => [p.id, p]))
    for (const n of knowledge.nodes) {
      const p = posMap.get(n.slug)
      if (!p) continue
      nodes.push({ id: n.slug, layer: 'knowledge', kind: n.type, title: n.title, x: p.x, y: p.y, z: p.z })
    }
    for (const e of knowledge.edges) {
      edges.push({ from: e.from_slug, to: e.to_slug, layer: 'knowledge' })
    }
  }

  return { nodes, edges, bridges: [] }
}

import { describe, it, expect } from 'vitest'
import { buildUnifiedScene } from './buildUnifiedScene'

const mem = {
  nodes: [
    { id: 'm1', spaceId: 'default', kind: 'boot', title: 'Boot', createdAt: '', updatedAt: '' },
    { id: 'm2', spaceId: 'default', kind: 'episode', title: 'Ep', createdAt: '', updatedAt: '' },
  ],
  edges: [{ id: 'e', spaceId: 'default', parentNodeId: 'm1', childNodeId: 'm2', relationKind: 'r', visibility: 'private', priority: 0, createdAt: '' }],
  routes: [],
} as any

const know = {
  nodes: [
    { slug: 'k1', title: 'Entity', type: 'entity' },
    { slug: 'k2', title: 'Concept', type: 'concept' },
  ],
  edges: [{ from_slug: 'k1', to_slug: 'k2', link_type: 'mentions' }],
}

describe('buildUnifiedScene', () => {
  it('maps both layers with layer tags', () => {
    const s = buildUnifiedScene(mem, know)
    expect(s.nodes.filter((n) => n.layer === 'memory')).toHaveLength(2)
    expect(s.nodes.filter((n) => n.layer === 'knowledge')).toHaveLength(2)
    expect(s.edges.filter((e) => e.layer === 'memory')).toHaveLength(1)
    expect(s.edges.filter((e) => e.layer === 'knowledge')).toHaveLength(1)
    expect(s.bridges).toHaveLength(0)
  })

  it('offsets memory cluster to -X and knowledge to +X', () => {
    const s = buildUnifiedScene(mem, know)
    const memNodes = s.nodes.filter((n) => n.layer === 'memory')
    const knowNodes = s.nodes.filter((n) => n.layer === 'knowledge')
    const memAvg = memNodes.reduce((a, n) => a + n.x, 0) / memNodes.length
    const knowAvg = knowNodes.reduce((a, n) => a + n.x, 0) / knowNodes.length
    expect(memAvg).toBeLessThan(0)
    expect(knowAvg).toBeGreaterThan(0)
  })

  it('handles either layer null', () => {
    expect(buildUnifiedScene(mem, null).nodes.every((n) => n.layer === 'memory')).toBe(true)
    expect(buildUnifiedScene(null, know).nodes.every((n) => n.layer === 'knowledge')).toBe(true)
    expect(buildUnifiedScene(null, null).nodes).toHaveLength(0)
  })

  it('knowledge node id is slug', () => {
    const s = buildUnifiedScene(null, know)
    expect(s.nodes.map((n) => n.id).sort()).toEqual(['k1', 'k2'])
  })
})

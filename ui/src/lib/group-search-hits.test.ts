import { describe, it, expect } from 'vitest'
import { groupHitsByWorkspace, type SearchHitWithWorkspace } from './group-search-hits'

interface Ws {
  id: string
  name: string
  icon: string
}

function hit(id: string, wsId: string | undefined): SearchHitWithWorkspace {
  return {
    id,
    title: `t-${id}`,
    snippet: '',
    source: 'agent_message',
    sourceId: `s-${id}`,
    workspaceId: wsId,
    createdAt: '2026-01-01',
  }
}

function ws(id: string, name: string): Ws {
  return { id, name, icon: 'Folder' }
}

describe('groupHitsByWorkspace', () => {
  it('groups hits by workspaceId', () => {
    const hits = [hit('1', 'ws-a'), hit('2', 'ws-b'), hit('3', 'ws-a')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(2)
    expect(groups[0].workspaceId).toBe('ws-a')
    expect(groups[0].hits).toHaveLength(2)
    expect(groups[1].workspaceId).toBe('ws-b')
    expect(groups[1].hits).toHaveLength(1)
  })

  it('puts the active workspace first, then workspaces-atom order', () => {
    const hits = [hit('1', 'ws-c'), hit('2', 'ws-a'), hit('3', 'ws-b')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B'), ws('ws-c', 'C')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-b')
    expect(groups.map((g) => g.workspaceId)).toEqual(['ws-b', 'ws-a', 'ws-c'])
  })

  it('omits workspaces with no hits', () => {
    const hits = [hit('1', 'ws-a')]
    const workspaces = [ws('ws-a', 'A'), ws('ws-b', 'B')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('ws-a')
  })

  it('treats missing workspaceId as "default"', () => {
    const hits = [hit('1', undefined)]
    const workspaces = [ws('default', '默认工作区'), ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('default')
    expect(groups[0].workspaceName).toBe('默认工作区')
  })

  it('caps each group at 5 visible hits and reports overflow', () => {
    const hits = Array.from({ length: 10 }, (_, i) => hit(String(i), 'ws-a'))
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups[0].visibleHits).toHaveLength(5)
    expect(groups[0].overflowCount).toBe(5)
  })

  it('has zero overflow when group has 5 or fewer hits', () => {
    const hits = [hit('1', 'ws-a'), hit('2', 'ws-a')]
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups[0].visibleHits).toHaveLength(2)
    expect(groups[0].overflowCount).toBe(0)
  })

  it('falls back to the workspace name "默认工作区" when not in the workspaces list', () => {
    const hits = [hit('1', 'ws-deleted')]
    const workspaces = [ws('ws-a', 'A')]
    const groups = groupHitsByWorkspace(hits, workspaces, 'ws-a')
    expect(groups).toHaveLength(1)
    expect(groups[0].workspaceId).toBe('ws-deleted')
    expect(groups[0].workspaceName).toBe('默认工作区')
  })
})

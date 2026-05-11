/**
 * Pure helper: group flat search hits by their originating workspace,
 * order the groups (active first, then workspace-bar order), and split
 * each group's hits into visible (top 5) + overflow count.
 *
 * Pure and synchronous so it's trivially unit-testable without touching
 * Jotai, React, or the Tauri bridge.
 */

export interface SearchHitWithWorkspace {
  id: string
  title: string
  snippet: string
  source: string
  sourceId: string
  messageId?: string
  workspaceId?: string
  createdAt: string
}

export interface SearchHitGroup {
  workspaceId: string
  workspaceName: string
  workspaceIcon: string
  hits: SearchHitWithWorkspace[]
  /** Top 5 hits; the rest go into overflow. */
  visibleHits: SearchHitWithWorkspace[]
  /** `hits.length - visibleHits.length`. Zero when no overflow. */
  overflowCount: number
}

interface WorkspaceLike {
  id: string
  name: string
  icon: string
}

const VISIBLE_PER_GROUP = 5
const FALLBACK_WORKSPACE_ID = 'default'
const FALLBACK_WORKSPACE_NAME = '默认工作区'
const FALLBACK_WORKSPACE_ICON = 'Folder'

export function groupHitsByWorkspace(
  hits: SearchHitWithWorkspace[],
  workspaces: WorkspaceLike[],
  activeWorkspaceId: string | null,
): SearchHitGroup[] {
  // Bucket by workspaceId (missing → 'default').
  const byWs = new Map<string, SearchHitWithWorkspace[]>()
  for (const h of hits) {
    const wsId = h.workspaceId ?? FALLBACK_WORKSPACE_ID
    if (!byWs.has(wsId)) byWs.set(wsId, [])
    byWs.get(wsId)!.push(h)
  }

  // Order: active workspace first, then workspaces-atom order, then any
  // orphans (workspaceIds present in hits but not in the workspaces list).
  const sortedKnown = workspaces.map((w) => w.id)
  const orderedKnown = [
    ...(activeWorkspaceId && sortedKnown.includes(activeWorkspaceId)
      ? [activeWorkspaceId]
      : []),
    ...sortedKnown.filter((id) => id !== activeWorkspaceId),
  ]
  const orphans = Array.from(byWs.keys()).filter(
    (id) => !sortedKnown.includes(id),
  )
  const orderedAll = [...orderedKnown, ...orphans]

  return orderedAll
    .filter((wsId) => byWs.has(wsId))
    .map((wsId) => {
      const ws = workspaces.find((w) => w.id === wsId)
      const hits = byWs.get(wsId) ?? []
      const visibleHits = hits.slice(0, VISIBLE_PER_GROUP)
      return {
        workspaceId: wsId,
        workspaceName: ws?.name ?? FALLBACK_WORKSPACE_NAME,
        workspaceIcon: ws?.icon ?? FALLBACK_WORKSPACE_ICON,
        hits,
        visibleHits,
        overflowCount: hits.length - visibleHits.length,
      }
    })
}

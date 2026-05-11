import { atom } from 'jotai'
import * as bridge from '@/lib/tauri-bridge'

export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
  attachedDirs: string[]
  sortOrder: number
  createdAt: string
  updatedAt: string
}

export interface WorkspaceSession {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  spaceId: string
  updatedAt: string
}

// All workspaces from backend
export const workspacesAtom = atom<WorkspaceInfo[]>([])

// Currently selected workspace ID
export const activeWorkspaceIdAtom = atom<string | null>(null)

// Sessions grouped by workspace id: { [workspaceId]: WorkspaceSession[] }
export const workspaceSessionsAtom = atom<Record<string, WorkspaceSession[]>>({})

// Action: refresh all workspaces from backend
export const refreshWorkspacesAtom = atom(
  null,
  async (_get, set) => {
    try {
      const spaces = await bridge.listSpaces()
      set(workspacesAtom, spaces as WorkspaceInfo[])
      const activeId = await bridge.getActiveWorkspaceId()
      if (activeId) set(activeWorkspaceIdAtom, activeId)
    } catch (e) {
      console.error('[workspace] failed to refresh workspaces', e)
    }
  }
)

// Action: update a workspace's name or icon
export const updateWorkspaceAtom = atom(
  null,
  async (_get, set, input: { id: string; name?: string; icon?: string }) => {
    await bridge.updateWorkspace(input)
    // Re-sync from backend rather than maintain optimistic state.
    const spaces = await bridge.listSpaces()
    set(workspacesAtom, spaces as WorkspaceInfo[])
  }
)

// Action: persist a new workspace order. Optimistic local update so the
// UI reflects the new order immediately on drop; reverts on backend
// failure to keep state honest.
export const reorderWorkspacesAtom = atom(
  null,
  async (get, set, orderedIds: string[]) => {
    const current = get(workspacesAtom)
    // Build optimistic reordered list: map ids → current entries, preserving
    // workspaces not in the orderedIds list (defensive; should be 1:1 normally).
    const lookup = new Map(current.map((w) => [w.id, w]))
    const reordered: WorkspaceInfo[] = []
    for (const id of orderedIds) {
      const w = lookup.get(id)
      if (w) reordered.push(w)
    }
    // Append any workspace not in orderedIds (shouldn't happen, but defensive).
    for (const w of current) {
      if (!orderedIds.includes(w.id)) reordered.push(w)
    }
    set(workspacesAtom, reordered)
    try {
      await bridge.reorderWorkspaces(orderedIds)
      // Re-sync from backend to get the authoritative state (also picks up
      // any concurrent changes another caller may have made).
      const spaces = await bridge.listSpaces()
      set(workspacesAtom, spaces as WorkspaceInfo[])
    } catch (err) {
      // Revert on failure.
      set(workspacesAtom, current)
      throw err
    }
  }
)

// Action: select a workspace and persist to backend
export const selectWorkspaceAtom = atom(
  null,
  async (_get, set, id: string) => {
    set(activeWorkspaceIdAtom, id)
    try {
      await bridge.setActiveWorkspaceId(id)
    } catch (e) {
      console.error('[workspace] failed to set active workspace', e)
    }
  }
)

// Partial update: update a single session's title/emoji in the grouped map
export const updateSessionTitleAtom = atom(
  null,
  (_get, set, { sessionId, title, emoji }: { sessionId: string; title: string; emoji: string }) => {
    set(workspaceSessionsAtom, (prev) => {
      const next = { ...prev }
      for (const spaceId of Object.keys(next)) {
        next[spaceId] = next[spaceId].map((s) =>
          s.id === sessionId
            ? { ...s, title, titleEmoji: emoji, titlePending: false }
            : s
        )
      }
      return next
    })
  }
)

// Action: sync agent sessions into workspace session map
export const syncWorkspaceSessionsAtom = atom(
  null,
  (_get, set, sessions: Array<{ id: string; workspaceId?: string; spaceId?: string; title?: string; titleEmoji?: string; titlePending?: boolean; updatedAt?: string; [key: string]: unknown }>) => {
    const grouped: Record<string, WorkspaceSession[]> = {}
    for (const s of sessions) {
      const spaceId = s.workspaceId ?? s.spaceId ?? 'default'
      if (!grouped[spaceId]) grouped[spaceId] = []
      // Parse metadataJson as fallback — but prefer direct fields which are kept up-to-date
      // by event listeners (session:title-updated, session:title-pending) via agentSessionsAtom.
      let meta: { title?: string; emoji?: string; title_pending?: boolean } = {}
      if (typeof s.metadataJson === 'string') {
        try { meta = JSON.parse(s.metadataJson) } catch { /* ignore */ }
      }
      grouped[spaceId].push({
        id: s.id,
        // Prefer direct fields (live-updated by atoms) over metadataJson (stale string)
        title: s.title ?? meta.title ?? 'New session',
        titleEmoji: s.titleEmoji ?? meta.emoji ?? '💬',
        titlePending: s.titlePending ?? meta.title_pending ?? false,
        spaceId,
        updatedAt: s.updatedAt ?? '',
      })
    }
    set(workspaceSessionsAtom, grouped)
  }
)

import { atom } from 'jotai'
import * as bridge from '@/lib/tauri-bridge'

export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
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

// Mark a session as title-pending (skeleton animation while LLM generates)
export const markSessionTitlePendingAtom = atom(
  null,
  (_get, set, sessionId: string) => {
    set(workspaceSessionsAtom, (prev) => {
      const next = { ...prev }
      for (const spaceId of Object.keys(next)) {
        next[spaceId] = next[spaceId].map((s) =>
          s.id === sessionId ? { ...s, titlePending: true } : s
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

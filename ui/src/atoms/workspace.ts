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
  /** ms timestamp; null means unpinned. */
  pinnedAt: number | null
  /** Raw metadata JSON blob from agent_sessions.metadata_json.
   *  Present for automation run-sessions (origin starts with "automation:").
   *  Used by WorkspaceRail to filter out automation sessions from the rail. */
  metadataJson?: string
  /** True when the session has been archived by the user. */
  archived?: boolean
  /** IM channel that originated this session ("wechat_ilink", "wecom_bot", …).
   *  Sourced from the im_sessions JOIN in list_agent_sessions; absent for
   *  in-app sessions. Drives the channel-icon override in SessionItem/TabBar. */
  imChannelType?: string
}

// All workspaces from backend
export const workspacesAtom = atom<WorkspaceInfo[]>([])

// Currently selected workspace ID
export const activeWorkspaceIdAtom = atom<string | null>(null)

/**
 * Active workspace's filesystem path, or null if no workspace has a
 * directory attached. Drives the cwd argument for every git IPC call
 * from BranchPicker / GitActionsPicker / GitWorkbenchDialog (W6 PR B).
 *
 * Pure derived atom — re-evaluates when `activeWorkspaceIdAtom` or
 * `workspacesAtom` changes. No IO, no async.
 */
export const activeWorkspaceCwdAtom = atom<string | null>((get) => {
  const id = get(activeWorkspaceIdAtom)
  if (!id) return null
  const ws = get(workspacesAtom).find((w) => w.id === id)
  return ws?.path ?? null
})

/**
 * Cross-surface git-branch sync tick.
 *
 * Bumped after any in-app action that changes the workspace's current
 * git branch (e.g. `SidebarGitActions`'s "create branch" flow). Consumers
 * who cache `gitCurrentBranch(cwd)` results in local React state include
 * this tick in their probe dependency array so they re-fetch and stay
 * in sync — needed because PR #132 moves `GitActionsPicker` to the
 * sidebar but `BranchPicker` (which displays the branch name) stays
 * in the composer.
 *
 * One atom + one extra `useEffect` dep — no event bus or pub-sub
 * needed for the current N=2 consumers.
 */
export const branchSyncTickAtom = atom<number>(0)

/**
 * Direction of the most-recent workspace switch — used by the UI to
 * pick which side an iOS-style slide animation enters from.
 * - `forward`  = moved to a later workspace in sortOrder → slide IN from right
 * - `backward` = moved to an earlier workspace          → slide IN from left
 *
 * Set inside `selectWorkspaceAtom` before flipping `activeWorkspaceIdAtom`
 * so consumers (LeftSidebar, TabBar, RightSidePanel) read the correct
 * direction on the same render where the new workspace becomes active.
 */
export const workspaceSwitchDirectionAtom = atom<'forward' | 'backward'>('forward')

/**
 * Active swipe-gesture state. Non-null while the user is actively
 * dragging the LeftSidebar to switch workspaces; null when at rest
 * (so AnimatePresence's normal cross-pass animation takes over).
 *
 * `offsetPx` is the *visual* translation of the current workspace
 * (after rubber-band damping). Positive = current slides RIGHT
 * (previous workspace peeks in from the LEFT); negative = current
 * slides LEFT (next workspace peeks in from the RIGHT).
 *
 * `previewWorkspaceId` is which workspace is currently being
 * revealed alongside — needed because the renderer can't recompute
 * direction every frame without knowing the gesture intent.
 */
export interface SwipeGestureState {
  offsetPx: number
  containerWidth: number
  previewWorkspaceId: string | null
}
export const swipeGestureAtom = atom<SwipeGestureState | null>(null)

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
// Also computes the slide direction (forward vs backward) BEFORE the
// active workspace flips, so the UI animates from the correct side.
//
// Callers that know the user's gesture direction (swipe / arrow keys
// that wrap around) can pass `{ id, direction }` to override the
// sortOrder-comparison heuristic — otherwise wrapping from the last
// workspace forward to the first would visually slide BACKWARD because
// the new sortOrder index is lower.
export const selectWorkspaceAtom = atom(
  null,
  async (get, set, input: string | { id: string; direction?: 'forward' | 'backward' }) => {
    const id = typeof input === 'string' ? input : input.id
    const dirOverride = typeof input === 'object' ? input.direction : undefined
    const prevId = get(activeWorkspaceIdAtom)
    if (prevId !== id) {
      if (dirOverride) {
        set(workspaceSwitchDirectionAtom, dirOverride)
      } else {
        const list = get(workspacesAtom)
        const prevIdx = list.findIndex((w) => w.id === prevId)
        const currIdx = list.findIndex((w) => w.id === id)
        if (prevIdx !== -1 && currIdx !== -1) {
          set(workspaceSwitchDirectionAtom, currIdx > prevIdx ? 'forward' : 'backward')
        }
      }
    }
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
        pinnedAt: typeof s.pinnedAt === 'number' ? s.pinnedAt : null,
        metadataJson: typeof s.metadataJson === 'string' ? s.metadataJson : undefined,
        archived: !!s.archived,
        imChannelType: typeof s.imChannelType === 'string' ? s.imChannelType : undefined,
      })
    }
    set(workspaceSessionsAtom, grouped)
  }
)

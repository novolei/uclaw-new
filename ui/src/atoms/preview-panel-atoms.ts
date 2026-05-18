/**
 * preview-panel-atoms — W4a preview panel state.
 *
 * selectedPreviewFileAtom — the file currently shown in the panel
 * previewPanelOpenAtom — whether the panel is visible
 * previewPanelSplitRatioAtom — chat ↔ preview horizontal split in MainArea
 * previewPanelWidthAtom — DEPRECATED (kept for compat; remove later wave)
 * openPreviewAction — atomic set-file + open
 * closePreviewAction — convenience
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import { dirtyBuffersAtom } from './preview-editor-atoms'

export interface PreviewFileTarget {
  /** Identifies the mount the file lives in (workspace:* / attached:*). */
  mountId: string
  /** Forward-slash path relative to the mount root. */
  relPath: string
  /** Display name (last segment of relPath). */
  name: string
  /** Optional session id — required for session-scoped mounts. */
  sessionId?: string | null
  /** Absolute on-disk path. Empty string if not yet resolved. */
  absolutePath?: string
}

// ── Multi-tab pool ─────────────────────────────────────────────────────
//
// Multi-file preview: the panel keeps a tab pool keyed by mountId:relPath.
// Agent-source tabs cluster left with a ✨ marker (the agent's outputs are
// what the user wants to inspect first); manual-source tabs cluster right.
// Re-opening the same file focuses the existing tab — never duplicates.
// In-memory only — matches the chat-session tabsAtom convention.

export type PreviewTabSource = 'agent' | 'manual'

export type PreviewTabType = 'file' | 'browser'

export interface PreviewTabItem {
  /** Composite identity: two tabs with same (mountId, relPath) are merged. */
  mountId: string
  relPath: string
  /** Display name (last path segment, mirror of PreviewFileTarget.name). */
  name: string
  /** Absolute path; undefined if unresolved (renderer handles that case). */
  absolutePath?: string
  /** Session that owns the mount, when relevant. */
  sessionId?: string | null
  /** Determines sort cluster + visual marker. */
  source: PreviewTabSource
  /** Insertion epoch — tiebreaker within source group. */
  addedAt: number
  /** Tab content type: file renderer or live browser panel. Default 'file'. */
  type: PreviewTabType
  /** Present only when type === 'browser'. */
  browser?: { agentSessionId: string; initialUrl: string }
}

export const previewTabsAtom = atom<PreviewTabItem[]>([])

/** `${mountId}:${relPath}` for the currently active tab, or null. */
export const activePreviewTabKeyAtom = atom<string | null>(null)

/** Stable composite key used everywhere the tab list is indexed. */
export function previewTabKey(
  t: Pick<PreviewTabItem, 'mountId' | 'relPath'>,
): string {
  return `${t.mountId}:${t.relPath}`
}

/** Agent tabs first (by addedAt asc), then manual (by addedAt asc). */
function sortPreviewTabs(tabs: PreviewTabItem[]): PreviewTabItem[] {
  return [...tabs].sort((a, b) => {
    if (a.source !== b.source) return a.source === 'agent' ? -1 : 1
    return a.addedAt - b.addedAt
  })
}

// ── selectedPreviewFileAtom — derived from active tab ──────────────────
//
// All existing readers of this atom (PreviewPanel, PreviewHeader,
// usePreviewState) continue to work; they see PreviewFileTarget | null
// just like before, but the source of truth is now the tab pool.

export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>((get) => {
  const key = get(activePreviewTabKeyAtom)
  if (!key) return null
  const tab = get(previewTabsAtom).find((t) => previewTabKey(t) === key)
  if (!tab) return null
  return {
    mountId: tab.mountId,
    relPath: tab.relPath,
    name: tab.name,
    absolutePath: tab.absolutePath,
    sessionId: tab.sessionId,
  }
})

export const previewPanelOpenAtom = atom<boolean>(false)

// ── Actions ────────────────────────────────────────────────────────────

/**
 * Open a file in a new tab, or focus the existing tab if already open.
 *
 * Source semantics:
 *  - 'agent'  → tab clusters left, carries ✨ marker. If a manual tab for
 *               the same file already exists, it gets promoted to 'agent'
 *               and migrates left.
 *  - 'manual' → tab clusters right. Re-opening an existing agent tab does
 *               NOT demote it (agent priority is sticky).
 *
 * Always sets the tab active and opens the panel.
 *
 * Note: the dirty-buffer confirm prompt that the legacy single-file
 * openPreviewAction used now lives on closePreviewTabAction (where it
 * belongs — switching tabs in a multi-tab pane should NOT discard buffers).
 */
export const openPreviewTabAction = atom(
  null,
  (
    get,
    set,
    payload: { target: PreviewFileTarget; source: PreviewTabSource },
  ) => {
    const tabs = get(previewTabsAtom)
    const key = previewTabKey(payload.target)
    const existing = tabs.find((t) => previewTabKey(t) === key)
    if (existing) {
      set(activePreviewTabKeyAtom, key)
      if (payload.source === 'agent' && existing.source === 'manual') {
        set(
          previewTabsAtom,
          sortPreviewTabs(
            tabs.map((t) =>
              previewTabKey(t) === key ? { ...t, source: 'agent' as const } : t,
            ),
          ),
        )
      }
      set(previewPanelOpenAtom, true)
      return
    }
    const tab: PreviewTabItem = {
      mountId: payload.target.mountId,
      relPath: payload.target.relPath,
      name: payload.target.name,
      absolutePath: payload.target.absolutePath,
      sessionId: payload.target.sessionId,
      source: payload.source,
      addedAt: Date.now(),
      type: 'file',
    }
    set(previewTabsAtom, sortPreviewTabs([...tabs, tab]))
    set(activePreviewTabKeyAtom, key)
    set(previewPanelOpenAtom, true)
  },
)

/**
 * Close a tab by its composite key.
 *  - If closing the active tab, activates the right neighbor (or left, or null)
 *  - If closing the last tab, also closes the panel
 *  - If the closing tab has a dirty editor buffer, prompts the user;
 *    on cancel, leaves the tab open
 *
 * No-op if the key isn't found.
 */
export const closePreviewTabAction = atom(
  null,
  (get, set, key: string) => {
    const tabs = get(previewTabsAtom)
    const idx = tabs.findIndex((t) => previewTabKey(t) === key)
    if (idx === -1) return
    const closingTab = tabs[idx]

    // Dirty-buffer confirmation (only when CLOSING this tab — switching
    // active tab between still-open tabs leaves buffers untouched).
    const buffers = get(dirtyBuffersAtom)
    const path = closingTab.absolutePath ?? null
    if (path && buffers.has(path)) {
      const proceed = window.confirm(
        '该文件有未保存的修改 — 关闭这个标签将丢弃这些修改。是否继续？',
      )
      if (!proceed) return
      const nextBuffers = new Map(buffers)
      nextBuffers.delete(path)
      set(dirtyBuffersAtom, nextBuffers)
    }

    const next = tabs.filter((t) => previewTabKey(t) !== key)
    set(previewTabsAtom, next)
    if (get(activePreviewTabKeyAtom) === key) {
      const neighbor = next[idx] ?? next[idx - 1] ?? null
      set(activePreviewTabKeyAtom, neighbor ? previewTabKey(neighbor) : null)
      if (next.length === 0) {
        set(previewPanelOpenAtom, false)
      }
    }
  },
)

/** Close every tab + collapse the panel. Used on workspace switch. */
export const clearAllPreviewTabsAction = atom(null, (_get, set) => {
  set(previewTabsAtom, [])
  set(activePreviewTabKeyAtom, null)
  set(previewPanelOpenAtom, false)
})

// ── Compatibility wrapper ──────────────────────────────────────────────
//
// Legacy callers do `set(openPreviewAction, target)`. They still work —
// new tabs default to source: 'manual'. New callers should use
// openPreviewTabAction explicitly with the right source.
export const openPreviewAction = atom(
  null,
  (_get, set, target: PreviewFileTarget) => {
    set(openPreviewTabAction, { target, source: 'manual' })
  },
)

/**
 * Persisted width in CSS pixels. Default 540; clamped to [380, 1100] by the UI.
 *
 * @deprecated The W4a-followup move-to-MainArea uses
 * `previewPanelSplitRatioAtom` instead, since the panel now shares the central
 * area with chat as a horizontal split rather than docking with a fixed width.
 * Kept for backwards compatibility; remove in a later wave once no consumer
 * reads it.
 */
export const previewPanelWidthAtom = atomWithStorage<number>(
  'uclaw-preview-panel-width',
  540,
)

/**
 * Persisted split ratio for the chat ↔ preview horizontal split in MainArea.
 *
 * Stored as the chat-side fraction (0.30 = chat is 30% wide, preview is 70%).
 * Clamped to [0.30, 0.80] by the resize handler so neither side disappears.
 * Default 0.55 — chat slightly wider than preview, mirroring Proma's default.
 */
export const previewPanelSplitRatioAtom = atomWithStorage<number>(
  'uclaw-preview-panel-split-ratio',
  0.55,
)

/**
 * Auto-preview toggle — when true, the preview panel opens automatically
 * when the agent writes or edits a file. Persisted to localStorage so the
 * user's preference survives reload. Mirrors Proma's `autoPreview` localStorage
 * key but scoped to Agent mode only (Chat mode has no tool calls).
 */
export const autoPreviewEnabledAtom = atomWithStorage<boolean>(
  'uclaw-auto-preview-enabled',
  true,
)

/**
 * Sessions where the user has manually dismissed the auto-opened preview
 * during the current turn. While a session id is in this set, auto-preview
 * stays quiet — the user already said "not now". Cleared by the next user
 * message for that session so the *next* turn's writes can pop the panel
 * again. Improves over Proma which reopens on every write.
 *
 * Not persisted — per-turn intent should not survive reload.
 */
export const autoPreviewDismissedSessionsAtom = atom<Set<string>>(new Set<string>())

/**
 * Map<sessionId, Map<toolCallId, absolutePath>> — write tool calls
 * currently in flight (tool_start seen, tool_result not yet). Drives the
 * progress indicator in `PreviewSurface` header so the user sees that the
 * agent is actively writing the file they're previewing.
 *
 * Not persisted — in-flight state has no meaning across reload.
 */
export const pendingWriteToolsAtom = atom<Map<string, Map<string, string>>>(new Map())

/**
 * Write-only action: close the panel, keep the selection for re-open.
 *
 * Also stamps the current target's session into autoPreviewDismissedSessionsAtom
 * so the auto-preview listener stays quiet for the rest of the turn. This is
 * the manual-dismiss path (close button + Esc); workspace-switch and similar
 * programmatic closes bypass this action by writing previewPanelOpenAtom
 * directly.
 */
export const closePreviewAction = atom(null, (get, set) => {
  const currentTarget = get(selectedPreviewFileAtom)
  const buffers = get(dirtyBuffersAtom)
  const currentPath = currentTarget?.absolutePath ?? null
  if (currentPath && buffers.has(currentPath)) {
    const proceed = window.confirm(
      '当前文件有未保存的修改 — 关闭预览将丢弃这些修改。是否继续？',
    )
    if (!proceed) return
    const next = new Map(buffers)
    next.delete(currentPath)
    set(dirtyBuffersAtom, next)
  }
  const sid = currentTarget?.sessionId ?? null
  if (sid) {
    set(autoPreviewDismissedSessionsAtom, (prev: Set<string>) => {
      if (prev.has(sid)) return prev
      const next = new Set(prev)
      next.add(sid)
      return next
    })
  }
  set(previewPanelOpenAtom, false)
  set(previewTabsAtom, [])              // NEW
  set(activePreviewTabKeyAtom, null)    // NEW
})

/**
 * Open a live browser panel tab for the given agent session.
 * Re-focuses existing browser tab for the same session without duplication.
 */
export const openBrowserTabAction = atom(
  null,
  (get, set, payload: { agentSessionId: string; initialUrl?: string }) => {
    const tabs = get(previewTabsAtom)
    const tab: PreviewTabItem = {
      mountId: 'browser',
      relPath: payload.agentSessionId,
      name: '浏览器',
      absolutePath: '',
      source: 'agent',
      addedAt: Date.now(),
      type: 'browser',
      browser: {
        agentSessionId: payload.agentSessionId,
        initialUrl: payload.initialUrl ?? '',
      },
    }
    const key = previewTabKey(tab)
    const existing = tabs.find((t) => previewTabKey(t) === key)
    if (existing) {
      set(activePreviewTabKeyAtom, key)
      set(previewPanelOpenAtom, true)
      return
    }
    set(previewTabsAtom, sortPreviewTabs([...tabs, tab]))
    set(activePreviewTabKeyAtom, key)
    set(previewPanelOpenAtom, true)
  },
)

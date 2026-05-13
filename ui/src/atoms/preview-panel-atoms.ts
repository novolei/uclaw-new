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

export const selectedPreviewFileAtom = atom<PreviewFileTarget | null>(null)
export const previewPanelOpenAtom = atom<boolean>(false)

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

/** Write-only action: select a file AND open the panel in one update. */
export const openPreviewAction = atom(null, (get, set, payload: PreviewFileTarget) => {
  const currentTarget = get(selectedPreviewFileAtom)
  const buffers = get(dirtyBuffersAtom)
  const currentPath = currentTarget?.absolutePath ?? null
  // Switching FROM a dirty file → confirm
  if (
    currentPath &&
    currentPath !== payload.absolutePath &&
    buffers.has(currentPath)
  ) {
    const proceed = window.confirm(
      '当前文件有未保存的修改 — 切换将丢弃这些修改。是否继续？',
    )
    if (!proceed) return
    // User chose to discard — clear the dirty entry so the next mount
    // doesn't see stale state.
    const next = new Map(buffers)
    next.delete(currentPath)
    set(dirtyBuffersAtom, next)
  }
  set(selectedPreviewFileAtom, payload)
  set(previewPanelOpenAtom, true)
})

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
export const autoPreviewDismissedSessionsAtom = atom<Set<string>>(new Set())

/**
 * Map<sessionId, Map<toolCallId, absolutePath>> — write tool calls
 * currently in flight (tool_start seen, tool_result not yet). Drives the
 * progress indicator in `PreviewSurface` header so the user sees that the
 * agent is actively writing the file they're previewing.
 *
 * Not persisted — in-flight state has no meaning across reload.
 */
export const pendingWriteToolsAtom = atom<Map<string, Map<string, string>>>(new Map())

/** Write-only action: close the panel, keep the selection for re-open. */
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
  set(previewPanelOpenAtom, false)
})

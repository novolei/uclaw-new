/**
 * preview-chip-atoms — Shared state for file-path chips (W4c).
 *
 * `chipResolutionCacheAtom` is read by every <FilePathChip> through
 * `useFileChipResolver`. It's a bounded LRU (cap 500) to keep memory
 * stable across long sessions.
 *
 * `addPendingAttachmentAction` is dispatched by Shift-click on a chip OR
 * on a FileTreeNode row. It eagerly fetches bytes (so the resulting
 * PendingAttachment carries `localPath` for downstream send paths) and
 * surfaces a sonner toast.
 */

import { atom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import type { PendingAttachment } from './chat-atoms'
import { pendingAttachmentsAtom } from './chat-atoms'

export type ChipResolutionState = 'pending' | 'ok' | 'missing'

export interface ChipResolutionEntry {
  state: ChipResolutionState
  mountId: string
  relPath: string
  absolutePath: string
}

const CACHE_MAX = 500

export const chipResolutionCacheAtom = atom<Map<string, ChipResolutionEntry>>(new Map())

export const setChipResolutionAction = atom(
  null,
  (get, set, payload: { rawPath: string; entry: ChipResolutionEntry }) => {
    const current = get(chipResolutionCacheAtom)
    const next = new Map(current)
    next.delete(payload.rawPath)
    next.set(payload.rawPath, payload.entry)
    while (next.size > CACHE_MAX) {
      const oldest = next.keys().next().value as string | undefined
      if (oldest === undefined) break
      next.delete(oldest)
    }
    set(chipResolutionCacheAtom, next)
  },
)

export const invalidateChipResolutionsAction = atom(
  null,
  (get, set, paths: string[]) => {
    if (paths.length === 0) return
    const current = get(chipResolutionCacheAtom)
    const next = new Map(current)
    let mutated = false
    for (const p of paths) {
      if (next.delete(p)) mutated = true
    }
    if (mutated) set(chipResolutionCacheAtom, next)
  },
)

// PreviewBytes serializes without serde rename_all — fields are snake_case.
interface PreviewBytesIpcPayload {
  resolved_path: string
  bytes: number[]
  size: number
  truncated: boolean
  mtime_ms: number
}

interface AddAttachmentPayload {
  mountId: string
  relPath: string
  name: string
  sessionId: string | null
  absolutePath: string
}

function inferMediaType(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) {
    return `image/${ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext}`
  }
  if (ext === 'pdf') return 'application/pdf'
  return 'text/plain'
}

export const addPendingAttachmentAction = atom(
  null,
  async (get, set, payload: AddAttachmentPayload) => {
    const dedupeKey = payload.absolutePath || `${payload.mountId}::${payload.relPath}`
    const current = get(pendingAttachmentsAtom)
    if (
      current.some(
        (a) => (a.localPath || a.filename) === dedupeKey || a.localPath === payload.absolutePath,
      )
    ) {
      toast.info('文件已在附件中', { id: 'attachment-added', description: payload.name })
      return
    }
    try {
      const result = await invoke<PreviewBytesIpcPayload>('preview_read_bytes', {
        mountId: payload.mountId,
        relPath: payload.relPath,
        sessionId: payload.sessionId ?? null,
      })
      const next: PendingAttachment = {
        filename: payload.name,
        localPath: result.resolved_path,
        mediaType: inferMediaType(payload.name),
        size: result.size,
      }
      set(pendingAttachmentsAtom, [...current, next])
      toast.success(`已添加 ${payload.name} 到聊天`, { id: 'attachment-added' })
    } catch (err) {
      toast.error('无法添加附件', {
        id: 'attachment-added',
        description: err instanceof Error ? err.message : String(err),
      })
    }
  },
)

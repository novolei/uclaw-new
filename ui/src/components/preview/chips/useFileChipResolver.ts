/**
 * useFileChipResolver — Batched async existence check for FilePathChip.
 *
 * Module-scope batching: every chip that mounts and hits an empty cache
 * entry queues its rawPath. The first queued path schedules a 50 ms
 * setTimeout to flush. The flush issues a single `preview_resolve_chips`
 * invoke and seeds the cache.
 *
 * The hook returns synchronously from the cache (or 'pending' placeholders),
 * relying on react-redraw when the cache atom updates.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  chipResolutionCacheAtom,
  setChipResolutionAction,
  type ChipResolutionEntry,
} from '@/atoms/preview-chip-atoms'

interface ChipResolutionIpcPayload {
  input: string
  exists: boolean
  mountId: string | null
  relPath: string | null
  absolutePath: string | null
}

// files_rail:change payload (ChangeKind serialises as snake_case).
interface FileChange {
  kind: 'created' | 'modified' | 'removed' | 'renamed'
  rel_path: string
  is_dir: boolean
}

interface FilesRailChangePayload {
  mount_id: string
  changes: FileChange[]
}

interface BatchEntry {
  rawPath: string
  sessionId: string | null
}

const BATCH_WINDOW_MS = 50
const queue: BatchEntry[] = []
let scheduled = false

function flush(setEntry: (p: { rawPath: string; entry: ChipResolutionEntry }) => void): void {
  if (queue.length === 0) {
    scheduled = false
    return
  }
  const batch = queue.splice(0, queue.length)
  scheduled = false
  // Group by sessionId — typically a single value, but be defensive.
  const groups = new Map<string | null, BatchEntry[]>()
  for (const item of batch) {
    const arr = groups.get(item.sessionId) ?? []
    arr.push(item)
    groups.set(item.sessionId, arr)
  }
  for (const [sessionId, items] of groups) {
    const paths = items.map((it) => it.rawPath)
    void invoke<ChipResolutionIpcPayload[]>('preview_resolve_chips', {
      paths,
      sessionId,
    })
      .then((results) => {
        const byInput = new Map(results.map((r) => [r.input, r]))
        for (const item of items) {
          const r = byInput.get(item.rawPath)
          const entry: ChipResolutionEntry = r
            ? {
                state: r.exists ? 'ok' : 'missing',
                mountId: r.mountId ?? '',
                relPath: r.relPath ?? '',
                absolutePath: r.absolutePath ?? '',
              }
            : { state: 'missing', mountId: '', relPath: '', absolutePath: '' }
          setEntry({ rawPath: item.rawPath, entry })
        }
      })
      .catch(() => {
        for (const item of items) {
          setEntry({
            rawPath: item.rawPath,
            entry: { state: 'missing', mountId: '', relPath: '', absolutePath: '' },
          })
        }
      })
  }
}

function enqueue(
  rawPath: string,
  sessionId: string | null,
  setEntry: (p: { rawPath: string; entry: ChipResolutionEntry }) => void,
): void {
  // Dedup: if the path is already queued (e.g. multiple chips for the same
  // file mounted in the same tick, or invalidator re-enqueued during flush),
  // don't push twice — the existing entry will resolve it.
  if (queue.some((q) => q.rawPath === rawPath)) return
  queue.push({ rawPath, sessionId })
  if (!scheduled) {
    scheduled = true
    setTimeout(() => flush(setEntry), BATCH_WINDOW_MS)
  }
}

const PENDING_ENTRY: ChipResolutionEntry = {
  state: 'pending',
  mountId: '',
  relPath: '',
  absolutePath: '',
}

/**
 * Subscribe to a single chip's resolution. Returns synchronously.
 * `'pending'` for un-cached paths; the hook triggers a backend resolve
 * and the cache update will re-render this chip.
 */
export function useFileChipResolver(
  rawPath: string,
  sessionId: string | null,
): ChipResolutionEntry {
  const cache = useAtomValue(chipResolutionCacheAtom)
  const setEntry = useSetAtom(setChipResolutionAction)
  const existing = cache.get(rawPath)
  // Re-react when cache state transitions to 'pending' (invalidator set it back)
  // so we re-enqueue a fetch — fixes the "stays pending after file delete" bug.
  const currentState = existing?.state

  React.useEffect(() => {
    if (!rawPath) return
    if (existing && existing.state !== 'pending') return
    if (!existing) setEntry({ rawPath, entry: PENDING_ENTRY })
    enqueue(rawPath, sessionId, setEntry)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawPath, sessionId, currentState])

  return existing ?? PENDING_ENTRY
}

/**
 * Top-level mount in MessageResponse: subscribes to files_rail:change events
 * once per session and invalidates Created/Removed cache entries by matching
 * relPath against cached entries.
 */
export function useChipCacheInvalidator(): void {
  const cache = useAtomValue(chipResolutionCacheAtom)
  const setEntry = useSetAtom(setChipResolutionAction)

  // Keep a stable ref to the cache so the listener closure sees the latest map.
  const cacheRef = React.useRef(cache)
  React.useEffect(() => {
    cacheRef.current = cache
  }, [cache])

  React.useEffect(() => {
    let cancelled = false
    let unlisten: undefined | (() => void)
    void listen<FilesRailChangePayload>('files_rail:change', (event) => {
      if (cancelled) return
      const { changes } = event.payload
      // Find rawPaths whose resolved relPath was affected by Created/Removed.
      for (const change of changes) {
        if (change.kind !== 'created' && change.kind !== 'removed') continue
        for (const [rawPath, entry] of cacheRef.current) {
          if (entry.relPath === change.rel_path || entry.absolutePath === rawPath) {
            // Bust by re-setting to pending so next render re-enqueues.
            setEntry({ rawPath, entry: PENDING_ENTRY })
          }
        }
      }
    }).then((fn) => {
      if (cancelled) {
        fn()
      } else {
        unlisten = fn
      }
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
    // setEntry is stable (write-only atom); intentionally omit cache from deps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setEntry])
}

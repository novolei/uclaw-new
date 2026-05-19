import { useEffect, useRef } from 'react'
import { useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { memuConsolidatingAtom } from '@/atoms/dock-atoms'

interface ConsolidationEvent {
  id: string
}

/**
 * Subscribe to memU consolidation start/finish events emitted by the Rust
 * MemUClient. Maintains a Set of in-flight consolidation ids and flips
 * `memuConsolidatingAtom` based on whether any are pending.
 *
 * Concurrent consolidations are de-duplicated: the atom only flips false
 * when the LAST in-flight call finishes. Defensive against:
 *   - finished events with no matching start (e.g. event-order race) — no-op
 *   - duplicate started events with the same id — Set.add idempotent
 *
 * Mount once at the app root (AppShell).
 */
export function useMemuConsolidation(): void {
  const setConsolidating = useSetAtom(memuConsolidatingAtom)
  const inFlightRef = useRef<Set<string>>(new Set())

  useEffect(() => {
    let active = true
    let unlistenStarted: (() => void) | null = null
    let unlistenFinished: (() => void) | null = null

    const updateAtom = () => {
      setConsolidating(inFlightRef.current.size > 0)
    }

    listen<ConsolidationEvent>('memu:consolidation_started', (e) => {
      if (!active) return
      inFlightRef.current.add(e.payload.id)
      updateAtom()
    }).then((fn) => {
      if (active) unlistenStarted = fn
      else fn()
    })

    listen<ConsolidationEvent>('memu:consolidation_finished', (e) => {
      if (!active) return
      inFlightRef.current.delete(e.payload.id)
      updateAtom()
    }).then((fn) => {
      if (active) unlistenFinished = fn
      else fn()
    })

    return () => {
      active = false
      if (unlistenStarted) unlistenStarted()
      if (unlistenFinished) unlistenFinished()
      inFlightRef.current.clear()
      setConsolidating(false)
    }
  }, [setConsolidating])
}

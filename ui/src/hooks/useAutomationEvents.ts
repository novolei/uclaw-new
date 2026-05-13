import { useEffect } from 'react'
import { useSetAtom } from 'jotai'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { pendingEscalationsAtom } from '@/atoms/automation'
import { listPendingEscalations } from '@/lib/tauri-bridge'
import type { EscalationRow } from '@/lib/tauri-bridge'

export function useAutomationEvents(): void {
  const setEscalations = useSetAtom(pendingEscalationsAtom)

  useEffect(() => {
    let cancelled = false

    // Initial fetch — survives restart and Phase 1 stub deferral. Surfaces any
    // rows lingering from previous Phase 2 sessions without waiting for an event.
    listPendingEscalations()
      .then((rows) => {
        if (!cancelled) setEscalations(rows)
      })
      .catch((err) => {
        console.error('[useAutomationEvents] initial fetch failed:', err)
      })

    // Future Phase 2 event subscription. The backend does not emit this event
    // today (Task 15 deferred InfraEvent extension); the subscription is here
    // so the wiring is in place when emitters are added.
    let unlisten: UnlistenFn | undefined
    listen<EscalationRow>('automation:escalation_raised', (event) => {
      setEscalations((prev) => {
        if (prev.some((e) => e.id === event.payload.id)) return prev
        return [...prev, event.payload]
      })
    })
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch((err) => {
        console.error('[useAutomationEvents] listen failed:', err)
      })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [setEscalations])
}

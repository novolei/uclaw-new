/**
 * Drives petPrimaryStateAtom from Tauri agent events + composer atoms.
 * Mount this ONCE at the app root (in App.tsx). Mounting multiple times
 * registers duplicate event listeners.
 *
 * Signal mapping (see spec):
 *   chat:stream-chunk     → thinking
 *   chat:stream-complete  → success (then auto-idle after 1500ms)
 *   chat:stream-error     → error
 *   agent:stream-reset    → idle
 *   composer focused + has text → typing (only if current is not thinking/success/error)
 */
import { listen } from '@tauri-apps/api/event'
import { useAtomValue, useSetAtom } from 'jotai'
import { useEffect, useRef } from 'react'
import { composerFocusedAtom, composerHasTextAtom } from '@/atoms/agent-atoms'
import { petPrimaryStateAtom, type PetPrimaryState } from '@/atoms/pet-atoms'

const SUCCESS_LINGER_MS = 1500

export function usePetStateSync(): void {
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const focused = useAtomValue(composerFocusedAtom)
  const hasText = useAtomValue(composerHasTextAtom)
  const successTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    let cancelled = false
    const unlistens: Array<() => void> = []

    const register = (eventName: string, handler: () => void) => {
      listen(eventName, handler).then((u) => {
        if (cancelled) u() // unmounted before promise resolved — immediately unlisten
        else unlistens.push(u)
      })
    }

    register('chat:stream-chunk', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('thinking')
    })
    register('chat:stream-complete', () => {
      setPrimary('success')
      if (successTimer.current) clearTimeout(successTimer.current)
      successTimer.current = setTimeout(() => {
        setPrimary('idle')
        successTimer.current = null
      }, SUCCESS_LINGER_MS)
    })
    register('chat:stream-error', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('error')
    })
    register('agent:stream-reset', () => {
      if (successTimer.current) {
        clearTimeout(successTimer.current)
        successTimer.current = null
      }
      setPrimary('idle')
    })

    return () => {
      cancelled = true
      if (successTimer.current) clearTimeout(successTimer.current)
      unlistens.forEach((u) => u())
    }
  }, [setPrimary])

  // Composer-driven typing transition: only override idle (never thinking/success/error)
  useEffect(() => {
    setPrimary((prev: PetPrimaryState): PetPrimaryState => {
      if (prev === 'thinking' || prev === 'success' || prev === 'error') return prev
      return focused && hasText ? 'typing' : 'idle'
    })
  }, [focused, hasText, setPrimary])
}

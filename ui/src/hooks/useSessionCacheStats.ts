/**
 * useSessionCacheStats — session-wide prompt-cache hit ratio for the
 * context usage indicator's "提示词缓存" section.
 *
 * Loads `get_session_cache_stats` on mount / session switch and re-fetches
 * (debounced) after every `agent:turn_cost` for this session, so the ratio
 * follows the turn as each LLM call is recorded. Failures keep the last
 * value — the indicator is informational and must never break the composer.
 */

import * as React from 'react'
import { getSessionCacheStats, onTurnCost } from '@/lib/tauri-bridge'
import type { SessionCacheStats } from '@/lib/types'

/** Default debounce after a turn_cost event (several calls per turn). */
const REFETCH_DEBOUNCE_MS = 500

/** Calls with zero cache reads before a provider counts as "未上报". The first
 *  call(s) of a session only WRITE the cache (cold cache, 0 reads), so a small
 *  count can't distinguish "cold" from "provider never reports reads". */
export const CACHE_UNREPORTED_MIN_CALLS = 3

/** Heuristic: several calls, never a cache read → the provider most likely
 *  doesn't report cache usage; its ratio is unknown, not 0%.
 *  TODO(ctx-continuity): replace with a backend reported/unknown signal
 *  (cost_records carries no cache-creation tokens, so cold vs unreported is
 *  guessed from the call count). */
export function isCacheUnreported(
  stats: SessionCacheStats | Pick<SessionCacheStats, 'calls' | 'cachedInputTokens'> | undefined,
): boolean {
  return stats != null
    && stats.calls >= CACHE_UNREPORTED_MIN_CALLS
    && stats.cachedInputTokens === 0
}

export function useSessionCacheStats(
  sessionId: string | undefined,
  debounceMs: number = REFETCH_DEBOUNCE_MS,
): SessionCacheStats | undefined {
  const [stats, setStats] = React.useState<SessionCacheStats | undefined>(undefined)

  React.useEffect(() => {
    setStats(undefined)
    if (!sessionId) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null

    const load = (): void => {
      getSessionCacheStats(sessionId)
        .then((s) => { if (!cancelled) setStats(s) })
        .catch((err: unknown) => {
          console.warn('[useSessionCacheStats] get_session_cache_stats failed:', err)
        })
    }
    load()

    const unlistenP = onTurnCost((payload) => {
      if (payload.conversationId !== sessionId) return
      if (timer) clearTimeout(timer)
      timer = setTimeout(load, debounceMs)
    })

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      void unlistenP.then((u) => u())
    }
  }, [sessionId, debounceMs])

  return stats
}

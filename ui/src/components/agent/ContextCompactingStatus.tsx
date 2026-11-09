/**
 * Inline "正在压缩上下文…" status for one agent session's transcript view
 * while an automatic (or provider-overflow) transcript compaction summarises
 * the older conversation — a blocking 10–60s LLM call during which nothing
 * else streams. Scoped to the view's own `sessionId` (AgentMessages is also
 * mounted for non-current sessions, e.g. automation views). Driven by the
 * session's `contextCompacting` stream flag (set on `agent:context_compacting`,
 * cleared on `agent:context_compacted` / `agent:context_compaction_failed`
 * and on turn end/error). Manual `/compact` keeps its own CompactingIndicator.
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { agentStreamingStatesAtom } from '@/atoms/agent-atoms'

export function ContextCompactingStatus({ sessionId }: { sessionId: string }): React.ReactElement | null {
  const state = useAtomValue(agentStreamingStatesAtom).get(sessionId)
  if (!state?.running || !state.contextCompacting) return null
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex items-center gap-1.5 my-2 pl-[46px] text-[12px] text-muted-foreground"
    >
      <Loader2 className="size-3 animate-spin shrink-0" />
      <span>正在压缩上下文…</span>
    </div>
  )
}

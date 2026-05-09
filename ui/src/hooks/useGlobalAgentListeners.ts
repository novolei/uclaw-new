import { useEffect } from 'react'
import { useStore } from 'jotai'

/** Jotai 没有公开导出 Store 类型，这里通过 useStore 的返回类型推导 */
type Store = ReturnType<typeof useStore>
import { listen } from '@tauri-apps/api/event'
import {
  agentStreamingStatesAtom,
  unviewedCompletedSessionIdsAtom,
  workingDoneSessionIdsAtom,
  agentStreamErrorsAtom,
  stoppedByUserSessionsAtom,
  currentAgentSessionIdAtom,
  agentSessionsAtom,
  proactiveLearningEventsAtom,
  type AgentStreamState,
  type ProactiveLearningEvent,
  type AgentStreamErrorPayload,
} from '@/atoms/agent-atoms'
import { workspaceSessionsAtom, updateSessionTitleAtom, type WorkspaceSession } from '@/atoms/workspace'
import { tabsAtom } from '@/atoms/tab-atoms'
import type { AgentSessionMeta } from '@/lib/agent-types'
import type { TabItem } from '@/atoms/tab-atoms'

function createInitialStreamState(): AgentStreamState {
  return {
    running: true,
    content: '',
    toolActivities: [],
    teammates: [],
    startedAt: Date.now(),
  }
}

// ─── Module-level singleton ───────────────────────────────────────────────────
// Listeners are global for the app's lifetime. Using a singleton prevents
// React StrictMode (which double-fires effects) and Vite HMR module reloads
// from stacking up duplicate Tauri event listeners.

let cleanupFns: Array<() => void> = []
let initialized = false

// Per-session last-processed seq numbers for chat:stream-reasoning deduplication.
// The backend includes a monotonically increasing `seq` with each delta; we skip
// any event whose seq is not strictly greater than the last one we processed.
// This defends against double-delivery that would otherwise cause word-by-word
// duplication in the streaming thinking block.
const lastReasoningSeq = new Map<string, number>()

function startAgentListeners(store: Store): void {
  if (initialized) return
  initialized = true

  // Helper: register a Tauri listener and collect its unlisten fn.
  // listen() is async, so we always store the unlisten fn once the Promise
  // settles — if dispose() already ran we call it immediately.
  let disposed = false
  function reg(p: Promise<() => void>): void {
    p.then((fn) => {
      if (disposed) fn()
      else cleanupFns.push(fn)
    }).catch(console.error)
  }

  // chat:stream-chunk → append streaming content
  reg(
    listen<{ conversationId: string; delta: string }>('chat:stream-chunk', ({ payload }) => {
      const sid = payload.conversationId
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const next = new Map(prev)
        next.set(sid, { ...existing, content: existing.content + payload.delta })
        return next
      })
      store.set(agentStreamErrorsAtom, (prev) => {
        if (!prev.has(sid)) return prev
        const next = new Map(prev)
        next.delete(sid)
        return next
      })
      store.set(stoppedByUserSessionsAtom, (prev) => {
        if (!prev.has(sid)) return prev
        const next = new Set(prev)
        next.delete(sid)
        return next
      })
    })
  )

  // chat:stream-complete → mark session done, finalize stuck activities
  reg(
    listen<{ conversationId: string; text: string }>('chat:stream-complete', ({ payload }) => {
      const sid = payload.conversationId
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid)
        if (!existing) return prev
        const next = new Map(prev)
        const finalActivities = existing.toolActivities.map((a) =>
          a.done ? a : { ...a, done: true }
        )
        next.set(sid, {
          ...existing,
          running: false,
          content: payload.text || existing.content,
          toolActivities: finalActivities,
        })
        return next
      })
      const currentSid = store.get(currentAgentSessionIdAtom)
      if (sid !== currentSid) {
        store.set(unviewedCompletedSessionIdsAtom, (prev) => {
          const next = new Set(prev)
          next.add(sid)
          return next
        })
      }
      store.set(workingDoneSessionIdsAtom, (prev) => {
        const next = new Set(prev)
        next.add(sid)
        return next
      })
    })
  )

  // chat:stream-error → record error and stop
  reg(
    listen<{
      conversationId: string
      error: string
      kind?: AgentStreamErrorPayload['kind']
      timeoutSecs?: number
    }>('chat:stream-error', ({ payload }) => {
      const sid = payload.conversationId
      store.set(agentStreamErrorsAtom, (prev) => {
        const next = new Map(prev)
        next.set(sid, {
          message: payload.error,
          kind: payload.kind,
          timeoutSecs: payload.timeoutSecs,
        })
        return next
      })
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid)
        if (!existing) return prev
        const next = new Map(prev)
        next.set(sid, { ...existing, running: false })
        return next
      })
    })
  )

  // chat:stream-reasoning → append thinking content
  reg(
    listen<{ conversationId: string; delta: string; seq?: number }>('chat:stream-reasoning', ({ payload }) => {
      const sid = payload.conversationId

      // Deduplicate: if the backend includes a seq number, skip events we've already processed.
      // Reset the tracked seq when a new stream starts (reasoning is undefined = fresh state).
      if (payload.seq !== undefined) {
        const currentReasoning = store.get(agentStreamingStatesAtom).get(sid)?.reasoning
        if (currentReasoning === undefined) {
          // New stream started — clear old seq so seq=0 is accepted again.
          lastReasoningSeq.delete(sid)
        }
        const last = lastReasoningSeq.get(sid)
        if (last !== undefined && payload.seq <= last) return
        lastReasoningSeq.set(sid, payload.seq)
      }

      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const next = new Map(prev)
        next.set(sid, { ...existing, reasoning: (existing.reasoning ?? '') + payload.delta })
        return next
      })
    })
  )

  // chat:stream-tool-activity → record tool activity
  reg(
    listen<{ conversationId: string; activity: any }>('chat:stream-tool-activity', ({ payload }) => {
      const sid = payload.conversationId
      const ev = payload.activity
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const activities = [...existing.toolActivities]

        if (ev.type === 'tool_start') {
          const newId = ev.toolCallId ?? ''
          if (!activities.some((a) => a.toolUseId === newId)) {
            activities.push({
              toolUseId: newId,
              toolName: ev.toolName ?? '',
              input: ev.input ?? {},
              done: false,
            })
          }
        } else if (ev.type === 'tool_result') {
          const idx = activities.findIndex((a) => a.toolUseId === ev.toolCallId)
          if (idx >= 0) {
            const raw = ev.result
            const resultStr: string =
              typeof raw === 'string'
                ? raw
                : (raw?.output ?? raw?.content ?? raw?.error ?? JSON.stringify(raw ?? ''))
            activities[idx] = {
              ...activities[idx]!,
              result: resultStr,
              isError: ev.isError ?? (raw?.ok === false),
              done: true,
            }
          }
        }

        const next = new Map(prev)
        next.set(sid, { ...existing, toolActivities: activities })
        return next
      })
    })
  )

  // session:title-pending → mark session title as generating (skeleton UI)
  reg(
    listen<string>('session:title-pending', ({ payload: sessionId }) => {
      // Update agentSessionsAtom
      store.set(agentSessionsAtom, (prev: AgentSessionMeta[]) =>
        prev.map((s: AgentSessionMeta) =>
          s.id === sessionId ? { ...s, titlePending: true } : s
        )
      )
      // Update workspaceSessionsAtom
      store.set(workspaceSessionsAtom, (prev: Record<string, WorkspaceSession[]>) => {
        const next = { ...prev }
        for (const spaceId of Object.keys(next)) {
          next[spaceId] = next[spaceId].map((s: WorkspaceSession) =>
            s.id === sessionId ? { ...s, titlePending: true } : s
          )
        }
        return next
      })
    })
  )

  // session:title-updated → apply generated title + emoji
  reg(
    listen<{ sessionId: string; title: string; emoji: string }>(
      'session:title-updated',
      ({ payload }) => {
        const { sessionId, title, emoji } = payload
        // Update agentSessionsAtom
        store.set(agentSessionsAtom, (prev: AgentSessionMeta[]) =>
          prev.map((s: AgentSessionMeta) =>
            s.id === sessionId
              ? { ...s, title, titleEmoji: emoji, titlePending: false }
              : s
          )
        )
        // Update workspaceSessionsAtom via the dedicated write-atom
        store.set(updateSessionTitleAtom, { sessionId, title, emoji })
        // Update tab bar: show emoji + title so the open tab reflects the new name
        const tabTitle = emoji ? `${emoji} ${title}` : title
        store.set(tabsAtom, (prev: TabItem[]) =>
          prev.map((t: TabItem) =>
            t.sessionId === sessionId ? { ...t, title: tabTitle } : t
          )
        )
      }
    )
  )

  // agent:proactive-learning → prepend to events list (cap at 10)
  reg(
    listen<ProactiveLearningEvent>('agent:proactive-learning', ({ payload }) => {
      store.set(proactiveLearningEventsAtom, (prev) =>
        [payload, ...prev].slice(0, 10)
      )
    })
  )

  // Dispose function: unlisten everything and reset for next HMR cycle
  const dispose = () => {
    disposed = true
    initialized = false
    for (const fn of cleanupFns) fn()
    cleanupFns = []
    lastReasoningSeq.clear()
  }

  // Vite HMR: tear down listeners before this module is hot-replaced so the
  // next module evaluation starts with a clean slate.
  if (import.meta.hot) {
    import.meta.hot.dispose(dispose)
  }
}

// ─── React hook ──────────────────────────────────────────────────────────────
// Just a mount trigger; the real work happens in startAgentListeners().
// StrictMode's double-run is harmless because startAgentListeners() guards
// against re-entry with the `initialized` flag.

export function useGlobalAgentListeners(): void {
  const store = useStore()

  useEffect(() => {
    startAgentListeners(store)
    // No cleanup returned — listeners are intentionally global for the app lifetime.
  }, [store])
}

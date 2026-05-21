/**
 * Per-session queue of messages the user typed while the agent was
 * already streaming. Codex-style: the message does NOT auto-send and
 * does NOT auto-interrupt. It sits in a banner above the composer
 * until the user explicitly:
 *
 *   - clicks "引导" (steer) — send now, interrupt current turn
 *   - clicks "编辑" (edit) — pop from queue, return to composer
 *   - clicks "删除" (trash) — discard
 *
 * OR the agent's current turn finishes naturally → the oldest queued
 * message is auto-dispatched (FIFO) without an interrupt.
 *
 * Persistence: localStorage via atomWithStorage so unsent steering
 * messages survive a hot-reload / window crash. Keyed by sessionId so
 * cross-session switching keeps each conversation's queue intact.
 */

import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export interface QueuedAgentMessage {
  /** Local UUID — used as React key + for surgical removal. */
  id: string
  /** Plain text user input. Attachments aren't supported (mirroring
   *  the existing streaming-append constraint in AgentView). */
  text: string
  /** ms epoch — used for FIFO ordering + dev UX (display "排队 30s"). */
  queuedAt: number
}

/** Map of sessionId → ordered queue (oldest first). */
export const agentQueuedMessagesMapAtom = atomWithStorage<
  Record<string, QueuedAgentMessage[]>
>('uclaw-agent-queued-messages-map', {})

/**
 * Helper: enqueue a message for a given session.
 *
 * Returns the new map — callers use this as the `set` payload of
 * `useSetAtom(agentQueuedMessagesMapAtom)`.
 */
export function enqueueAgentMessage(
  prev: Record<string, QueuedAgentMessage[]>,
  sessionId: string,
  text: string,
): Record<string, QueuedAgentMessage[]> {
  if (!text.trim()) return prev
  const existing = prev[sessionId] ?? []
  return {
    ...prev,
    [sessionId]: [
      ...existing,
      {
        id: crypto.randomUUID(),
        text,
        queuedAt: Date.now(),
      },
    ],
  }
}

/** Helper: remove one queued message by id. */
export function removeQueuedMessage(
  prev: Record<string, QueuedAgentMessage[]>,
  sessionId: string,
  id: string,
): Record<string, QueuedAgentMessage[]> {
  const existing = prev[sessionId] ?? []
  const filtered = existing.filter((m) => m.id !== id)
  if (filtered.length === existing.length) return prev
  if (filtered.length === 0) {
    const { [sessionId]: _, ...rest } = prev
    return rest
  }
  return { ...prev, [sessionId]: filtered }
}

/** Helper: pop the oldest queued message for FIFO auto-dispatch. */
export function popOldestQueuedMessage(
  prev: Record<string, QueuedAgentMessage[]>,
  sessionId: string,
): { next: Record<string, QueuedAgentMessage[]>; popped: QueuedAgentMessage | null } {
  const existing = prev[sessionId] ?? []
  if (existing.length === 0) return { next: prev, popped: null }
  const [popped, ...rest] = existing
  if (rest.length === 0) {
    const { [sessionId]: _, ...withoutKey } = prev
    return { next: withoutKey, popped: popped! }
  }
  return { next: { ...prev, [sessionId]: rest }, popped: popped! }
}

/**
 * Per-session derived atom: returns the queued messages for a
 * given session id. Components use this read-only.
 */
export const agentQueuedMessagesForAtom = atom((get) => {
  const map = get(agentQueuedMessagesMapAtom)
  return (sessionId: string | undefined | null): QueuedAgentMessage[] => {
    if (!sessionId) return []
    return map[sessionId] ?? []
  }
})

/**
 * Per-session agent display name registry.
 *
 * Slice 1 follow-up — unifies the assistant-message header across Chat
 * and Agent modes by giving each session a configurable display name.
 *
 * Storage: localStorage via `atomWithStorage` so user-set names survive
 * across restarts. Keys are conversationId strings; missing key falls
 * back to `DEFAULT_AGENT_NAME`.
 *
 * Future M2-J UI: a "rename agent" dialog will write into this atom;
 * for now, callers can `setAgentDisplayNameMap` directly from any
 * settings panel.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

/** Default agent name when no per-session override is set. */
export const DEFAULT_AGENT_NAME = 'uClaw'

/** Map of conversationId → user-set agent name. */
export const agentDisplayNameMapAtom = atomWithStorage<Record<string, string>>(
  'uclaw-agent-display-name-map',
  {},
)

/**
 * Read-only derived atom factory: returns a getter that resolves
 * `conversationId` to its display name with fallback to default.
 *
 * Usage:
 *   const lookup = useAtomValue(agentDisplayNameForAtom)
 *   const name = lookup(conversationId)   // "uClaw" or user-set
 */
export const agentDisplayNameForAtom = atom((get) => {
  const map = get(agentDisplayNameMapAtom)
  return (conversationId: string | undefined | null): string => {
    if (!conversationId) return DEFAULT_AGENT_NAME
    return map[conversationId] ?? DEFAULT_AGENT_NAME
  }
})

/** Convenience setter for renaming an agent on one session. */
export const setAgentDisplayName = (
  map: Record<string, string>,
  conversationId: string,
  name: string,
): Record<string, string> => {
  const trimmed = name.trim()
  if (!trimmed) {
    // Empty name → remove the override, falls back to default.
    const { [conversationId]: _, ...rest } = map
    return rest
  }
  return { ...map, [conversationId]: trimmed }
}

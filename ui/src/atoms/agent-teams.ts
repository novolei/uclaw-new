import { atom } from 'jotai'
import type { TeamChannelMessage } from '@/lib/tauri-bridge'

export type NodeStatus = 'idle' | 'running' | 'done' | 'failed'

export interface TeamNode {
  id: string
  role: 'supervisor' | 'worker' | 'reviewer'
  label: string
  status: NodeStatus
  lastMessage?: string
}

export interface TeamState {
  teamId: string
  sessionId: string
  task: string
  nodes: TeamNode[]
  messages: TeamChannelMessage[]
  status: 'idle' | 'running' | 'done' | 'failed'
}

// Active team run for current session
export const activeTeamAtom = atom<TeamState | null>(null)

// Whether teams panel is visible
export const teamsPanelOpenAtom = atom(false)

// Action: update node status from team events
export const updateTeamNodeAtom = atom(
  null,
  (_get, set, { teamId, nodeId, status, message }: { teamId: string; nodeId: string; status: NodeStatus; message?: string }) => {
    set(activeTeamAtom, (prev) => {
      if (!prev || prev.teamId !== teamId) return prev
      const found = prev.nodes.some((n) => n.id === nodeId)
      if (!found && import.meta.env.DEV) {
        console.warn(`[teams] updateTeamNodeAtom: node "${nodeId}" not found in team "${teamId}"`)
      }
      return {
        ...prev,
        nodes: prev.nodes.map((n) =>
          n.id === nodeId ? { ...n, status, lastMessage: message ?? n.lastMessage } : n
        ),
      }
    })
  }
)

// Action: append channel message
export const appendTeamMessageAtom = atom(
  null,
  (_get, set, msg: TeamChannelMessage) => {
    set(activeTeamAtom, (prev) => {
      if (!prev) return prev
      return { ...prev, messages: [...prev.messages, msg] }
    })
  }
)

// Active plan file shown in the Plan tab
export const activePlanAtom = atom<{ filename: string; content: string } | null>(null)

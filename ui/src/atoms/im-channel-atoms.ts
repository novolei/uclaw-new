import { atom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'

export interface ImChannelRow {
  id: string
  spaceId: string
  channelType: string
  name: string
  config: Record<string, unknown>
  enabled: boolean
  streaming: boolean
  replyScope: string
  permissionEnabled: boolean
  owners: string[]
  guestPolicy: { tool_allowlist: string[]; mcp_enabled: boolean }
  createdAt: number
  updatedAt: number
}

export interface ImChannelInput {
  spaceId: string
  channelType: string
  name: string
  config: Record<string, unknown>
  credentials: Record<string, unknown>
  enabled: boolean
  streaming: boolean
  replyScope: string
  permissionEnabled: boolean
  owners: string[]
  guestPolicy: Record<string, unknown>
}

/** Runtime connection status for one channel instance. */
export interface ImChannelStatus {
  instanceId: string
  state: 'online' | 'error' | 'offline' | 'needs_rebind'
  lastError?: string
  /** Epoch ms when WebSocket connected. Defined only when state === 'online'. */
  connectedSinceMs?: number
  /** Messages received today (resets on restart). */
  messageCountToday?: number
}

export const imChannelsAtom = atom<ImChannelRow[]>([])

export const fetchImChannelsAtom = atom(null, async (_get, set) => {
  const rows = await invoke<ImChannelRow[]>('list_im_channels')
  set(imChannelsAtom, rows)
})

/** Map of instanceId → ImChannelStatus. Updated by IPC events and initial fetch. */
export const imChannelStatusesAtom = atom<Record<string, ImChannelStatus>>({})

export const fetchImChannelStatusesAtom = atom(null, async (_get, set) => {
  const statuses = await invoke<ImChannelStatus[]>('get_im_channel_statuses')
  const map: Record<string, ImChannelStatus> = {}
  for (const s of statuses) map[s.instanceId] = s
  set(imChannelStatusesAtom, map)
})

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

export const imChannelsAtom = atom<ImChannelRow[]>([])

export const fetchImChannelsAtom = atom(null, async (_get, set) => {
  const rows = await invoke<ImChannelRow[]>('list_im_channels')
  set(imChannelsAtom, rows)
})

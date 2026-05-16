import { atom } from 'jotai'

// ─── 语音记忆录制状态机 ───────────────────────────────────────────

export type MemoryVoiceState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'recording'; interimText: string; volume: number }
  | { kind: 'saving'; text: string }
  | { kind: 'saved'; title: string; subtype: string }
  | { kind: 'permission-denied' }
  | { kind: 'error'; message: string }

/** 语音记忆录制状态 */
export const memoryVoiceStateAtom = atom<MemoryVoiceState>({ kind: 'idle' })

/** 语音记忆是否活跃（用于与 STT 互斥检查） */
export const memoryVoiceActiveAtom = atom<boolean>(
  (get) => get(memoryVoiceStateAtom).kind !== 'idle'
)

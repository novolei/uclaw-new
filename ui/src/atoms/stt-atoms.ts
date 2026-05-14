/**
 * stt-atoms — Jotai state for the STT feature.
 *
 * - `sttModalStateAtom`: the finite-state machine the streaming modal reads.
 * - `activeComposerAtom`: cross-composer lock — only one composer can record at a time.
 * - `sttSettingsAtom`: user-tunable settings; persisted to localStorage via atomWithStorage.
 * - `modelStatusAtom`: cached model-readiness result from `stt_model_status`.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type ComposerKind = 'chat' | 'agent'

/**
 * 流式语音 modal 的状态机。替代 RecordingState（Task 12 删除旧的）。
 * modal 在 kind !== 'idle' 时挂载。
 */
export type SttModalState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'listening'; segmentStartedMs: number; volume: number; interimText: string }
  | { kind: 'finalizing'; volume: number }
  | { kind: 'permission-denied' }
  | { kind: 'error'; message: string }

export type Language = 'auto' | 'zh' | 'en' | 'yue' | 'ja' | 'ko'

export interface SttSettings {
  language: Language
  microphoneDeviceId: string | null
  /** 静音多久（ms）触发段定稿。默认 1800。 */
  silenceThresholdMs: number
}

const DEFAULT_SETTINGS: SttSettings = {
  language: 'auto',
  microphoneDeviceId: null,
  silenceThresholdMs: 1800,
}

export type ModelStatus =
  | { kind: 'unknown' }
  | { kind: 'not-downloaded'; expectedDir: string }
  | { kind: 'downloading'; file: string; downloaded: number; total: number | null; percent: number }
  | { kind: 'ready'; modelDir: string }
  | { kind: 'error'; message: string }

export const activeComposerAtom = atom<ComposerKind | null>(null)
export const sttSettingsAtom = atomWithStorage<SttSettings>('uclaw.stt.settings', DEFAULT_SETTINGS)
export const modelStatusAtom = atom<ModelStatus>({ kind: 'unknown' })
export const sttModalStateAtom = atom<SttModalState>({ kind: 'idle' })

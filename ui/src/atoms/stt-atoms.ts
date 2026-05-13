/**
 * stt-atoms — Jotai state for the STT feature.
 *
 * - `recordingStateAtom`: the finite-state machine the inline recorder reads.
 * - `activeComposerAtom`: cross-composer lock — only one composer can record at a time.
 * - `sttSettingsAtom`: user-tunable settings; persisted to localStorage now,
 *    will round-trip to backend stt_save_settings in Task 15.
 * - `modelStatusAtom`: cached model-readiness result from `stt_model_status`.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type ComposerKind = 'chat' | 'agent'

export type RecordingState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'recording'; startedAtMs: number; volume: number }
  | { kind: 'transcribing' }
  | { kind: 'done'; text: string }
  | { kind: 'error'; message: string }
  | { kind: 'permission-denied' }

export type Language = 'auto' | 'zh' | 'en' | 'yue' | 'ja' | 'ko'

export interface SttSettings {
  language: Language
  autoSend: boolean
  microphoneDeviceId: string | null
}

const DEFAULT_SETTINGS: SttSettings = {
  language: 'auto',
  autoSend: false,
  microphoneDeviceId: null,
}

export type ModelStatus =
  | { kind: 'unknown' }
  | { kind: 'not-downloaded'; expectedDir: string }
  | { kind: 'downloading'; file: string; downloaded: number; total: number | null; percent: number }
  | { kind: 'ready'; modelDir: string }
  | { kind: 'error'; message: string }

export const recordingStateAtom = atom<RecordingState>({ kind: 'idle' })
export const activeComposerAtom = atom<ComposerKind | null>(null)
export const sttSettingsAtom = atomWithStorage<SttSettings>('uclaw.stt.settings', DEFAULT_SETTINGS)
export const modelStatusAtom = atom<ModelStatus>({ kind: 'unknown' })

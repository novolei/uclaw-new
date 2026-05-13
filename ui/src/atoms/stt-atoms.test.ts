import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  recordingStateAtom,
  activeComposerAtom,
  sttSettingsAtom,
  modelStatusAtom,
  type RecordingState,
  type SttSettings,
  type ModelStatus,
} from './stt-atoms'

describe('stt-atoms', () => {
  it('recordingStateAtom defaults to idle', () => {
    const store = createStore()
    const state: RecordingState = store.get(recordingStateAtom)
    expect(state.kind).toBe('idle')
  })

  it('activeComposerAtom defaults to null (single-session lock)', () => {
    const store = createStore()
    expect(store.get(activeComposerAtom)).toBeNull()
    // Lock to chat
    store.set(activeComposerAtom, 'chat')
    expect(store.get(activeComposerAtom)).toBe('chat')
    // Release
    store.set(activeComposerAtom, null)
    expect(store.get(activeComposerAtom)).toBeNull()
  })

  it('sttSettingsAtom has sensible defaults', () => {
    const store = createStore()
    const s: SttSettings = store.get(sttSettingsAtom)
    expect(s.language).toBe('auto')
    expect(s.autoSend).toBe(false)
    expect(s.microphoneDeviceId).toBeNull()
  })

  it('modelStatusAtom defaults to unknown then can be set to ready', () => {
    const store = createStore()
    const initial: ModelStatus = store.get(modelStatusAtom)
    expect(initial.kind).toBe('unknown')
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/home/x/.uclaw/models/sensevoice' })
    expect(store.get(modelStatusAtom).kind).toBe('ready')
  })
})

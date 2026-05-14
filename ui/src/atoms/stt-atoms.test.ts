import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import {
  activeComposerAtom,
  sttSettingsAtom,
  modelStatusAtom,
  sttModalStateAtom,
  type SttSettings,
  type ModelStatus,
  type SttModalState,
} from './stt-atoms'

describe('stt-atoms', () => {
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

describe('sttModalStateAtom', () => {
  it('defaults to idle', () => {
    const store = createStore()
    expect(store.get(sttModalStateAtom)).toEqual({ kind: 'idle' })
  })

  it('can transition to listening', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening',
      segmentStartedMs: 1000,
      volume: 0,
      interimText: '',
    })
    const s = store.get(sttModalStateAtom)
    expect(s.kind).toBe('listening')
  })
})

describe('sttSettingsAtom silenceThresholdMs', () => {
  it('defaults silenceThresholdMs to 1800', () => {
    const store = createStore()
    expect(store.get(sttSettingsAtom).silenceThresholdMs).toBe(1800)
  })
})

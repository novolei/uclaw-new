import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { installAudioStubs, type InstalledStubs } from '@/test-utils/stt-mocks'
import {
  startRecording,
  stopAndEncode,
  cancelRecording,
  type ActiveRecording,
} from './audio-capture'

let stubs: InstalledStubs

beforeEach(() => {
  stubs = installAudioStubs()
})
afterEach(() => {
  stubs.cleanup()
})

describe('audio-capture', () => {
  it('startRecording returns a handle with a non-null stream and analyser', async () => {
    const rec: ActiveRecording = await startRecording({ deviceId: null })
    expect(rec.stream).not.toBeNull()
    expect(rec.analyser).not.toBeNull()
    expect(rec.startedAtMs).toBeGreaterThan(0)
  })

  it('readVolume samples from AnalyserNode in 0..1 range', async () => {
    const rec = await startRecording({ deviceId: null })
    stubs.setVolume(128) // ~half-peak
    const v = rec.readVolume()
    expect(v).toBeGreaterThan(0.3)
    expect(v).toBeLessThanOrEqual(1)
  })

  it('stopAndEncode produces base64 PCM16LE non-empty when chunks were pushed', async () => {
    const rec = await startRecording({ deviceId: null })
    // Simulate 1 chunk of audio (small webm blob)
    stubs.emitData(new Blob([new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8])], { type: 'audio/webm' }))
    const result = await stopAndEncode(rec)
    expect(result.audioBytesBase64.length).toBeGreaterThan(0)
    expect(result.sampleRate).toBe(16000)
  })

  it('cancelRecording stops without producing data', async () => {
    const rec = await startRecording({ deviceId: null })
    cancelRecording(rec)
    expect(rec.mediaRecorder.state).toBe('inactive')
  })
})

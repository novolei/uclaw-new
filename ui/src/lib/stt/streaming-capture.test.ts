import { describe, it, expect, afterEach } from 'vitest'
import { installAudioStubs, type InstalledStubs } from '@/test-utils/stt-mocks'
import { createStreamingCapture } from './streaming-capture'

let stubs: InstalledStubs

afterEach(() => {
  stubs?.cleanup()
})

describe('createStreamingCapture', () => {
  it('accumulates posted PCM and returns non-empty base64 for the segment', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.emitPcm(new Float32Array(128).fill(0.5))
    stubs.emitPcm(new Float32Array(128).fill(-0.5))
    const b64 = cap.getSegmentPcmBase64()
    expect(typeof b64).toBe('string')
    expect(b64.length).toBeGreaterThan(0)
    cap.stop()
  })

  it('resetSegment clears the accumulated buffer', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.emitPcm(new Float32Array(128).fill(0.5))
    const before = cap.getSegmentPcmBase64()
    cap.resetSegment()
    const after = cap.getSegmentPcmBase64()
    expect(after).toBe('')
    expect(before).not.toBe('')
    cap.stop()
  })

  it('getVolume returns a number in 0..1', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.setVolume(128)
    const v = cap.getVolume()
    expect(v).toBeGreaterThanOrEqual(0)
    expect(v).toBeLessThanOrEqual(1)
    cap.stop()
  })

  it('getFrequencyBands returns the requested number of 0..1 values', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.setVolume(160)
    const bands = cap.getFrequencyBands(7)
    expect(bands).toHaveLength(7)
    for (const b of bands) {
      expect(b).toBeGreaterThanOrEqual(0)
      expect(b).toBeLessThanOrEqual(1)
    }
    cap.stop()
  })
})

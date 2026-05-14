/**
 * stt-mocks — jsdom-safe mocks for AudioContext / AudioWorkletNode / mediaDevices.
 *
 * jsdom (vitest's default env) doesn't ship browser audio APIs, so STT-related
 * tests must stub them. These helpers install minimal-but-realistic mocks on
 * `globalThis` and return cleanup functions.
 */
import { vi } from 'vitest'

export interface InstalledStubs {
  emitPcm: (pcm: Float32Array) => void
  setVolume: (v: number) => void
  cleanup: () => void
}

export function installAudioStubs(): InstalledStubs {
  let volumeByte = 0
  const workletPorts: Array<{ onmessage: ((e: MessageEvent) => void) | null }> = []

  // ── AudioWorkletNode (mock) ──────────────────────────────────────────────
  class MockAudioWorkletNode {
    port: { onmessage: ((e: MessageEvent) => void) | null }
    constructor() {
      this.port = { onmessage: null }
      workletPorts.push(this.port)
    }
    connect() {}
    disconnect() {}
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(globalThis as any).AudioWorkletNode = MockAudioWorkletNode

  // ── AudioContext (only what streaming-capture.ts uses) ──────────────────
  class MockAnalyser {
    fftSize = 256
    frequencyBinCount = 128
    getByteFrequencyData(arr: Uint8Array) {
      arr.fill(volumeByte)
    }
  }
  class MockAudioContext {
    sampleRate = 16000
    state: AudioContextState = 'running'
    audioWorklet = { addModule: (_url: string) => Promise.resolve() }
    createAnalyser() {
      return new MockAnalyser()
    }
    createMediaStreamSource(_s: MediaStream) {
      return { connect: () => {} }
    }
    decodeAudioData(_arr: ArrayBuffer): Promise<AudioBuffer> {
      // Return a 16kHz mono 1-second silence buffer
      const mockBuffer = {
        sampleRate: 16000,
        numberOfChannels: 1,
        length: 16000,
        getChannelData: () => new Float32Array(16000),
      } as unknown as AudioBuffer
      return Promise.resolve(mockBuffer)
    }
    close() {
      return Promise.resolve()
    }
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(globalThis as any).AudioContext = MockAudioContext

  // ── navigator.mediaDevices ──────────────────────────────────────────────
  const fakeStream = {
    getTracks: () => [{ stop: vi.fn() }],
  } as unknown as MediaStream
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: {
      getUserMedia: vi.fn().mockResolvedValue(fakeStream),
      enumerateDevices: vi.fn().mockResolvedValue([
        { deviceId: 'default', kind: 'audioinput', label: 'Default Mic' },
      ]),
    },
  })

  return {
    emitPcm(pcm: Float32Array) {
      workletPorts.forEach((p) => p.onmessage?.({ data: pcm } as MessageEvent))
    },
    setVolume(v: number) {
      volumeByte = Math.max(0, Math.min(255, v))
    },
    cleanup() {
      workletPorts.length = 0
      Object.defineProperty(navigator, 'mediaDevices', {
        configurable: true,
        value: undefined,
      })
    },
  }
}

/**
 * stt-mocks — jsdom-safe mocks for MediaRecorder / AudioContext / AnalyserNode.
 *
 * jsdom (vitest's default env) doesn't ship browser audio APIs, so STT-related
 * tests must stub them. These helpers install minimal-but-realistic mocks on
 * `globalThis` and return cleanup functions.
 */
import { vi } from 'vitest'

export interface InstalledStubs {
  emitData: (chunk: Blob) => void
  emitStop: () => void
  setVolume: (v: number) => void
  cleanup: () => void
}

export function installAudioStubs(): InstalledStubs {
  const dataListeners: Array<(e: BlobEvent) => void> = []
  const stopListeners: Array<() => void> = []
  let volumeByte = 0

  // ── MediaRecorder ───────────────────────────────────────────────────────
  class MockMediaRecorder {
    state: 'inactive' | 'recording' | 'paused' = 'inactive'
    ondataavailable: ((e: BlobEvent) => void) | null = null
    onstop: (() => void) | null = null
    mimeType: string
    constructor(_stream: MediaStream, opts?: MediaRecorderOptions) {
      this.mimeType = opts?.mimeType ?? 'audio/webm'
    }
    start() {
      this.state = 'recording'
    }
    stop() {
      this.state = 'inactive'
      const stopFn = () => {
        if (this.onstop) this.onstop()
        stopListeners.forEach((l) => l())
      }
      stopFn()
    }
    addEventListener(ev: string, cb: EventListenerOrEventListenerObject) {
      if (ev === 'dataavailable')
        dataListeners.push(cb as (e: BlobEvent) => void)
      if (ev === 'stop') stopListeners.push(cb as () => void)
    }
    static isTypeSupported(_t: string) {
      return true
    }
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(globalThis as any).MediaRecorder = MockMediaRecorder
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(globalThis as any).BlobEvent = Event

  // ── AudioContext (only what audio-capture.ts uses) ──────────────────────
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
    emitData(chunk: Blob) {
      dataListeners.forEach((l) =>
        l({ data: chunk } as unknown as BlobEvent),
      )
    },
    emitStop() {
      stopListeners.forEach((l) => l())
    },
    setVolume(v: number) {
      volumeByte = Math.max(0, Math.min(255, v))
    },
    cleanup() {
      dataListeners.length = 0
      stopListeners.length = 0
      Object.defineProperty(navigator, 'mediaDevices', {
        configurable: true,
        value: undefined,
      })
    },
  }
}

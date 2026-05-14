/**
 * streaming-capture — 边录边累积 PCM 的音频采集（伪流式用）。
 *
 * getUserMedia → AudioContext(16kHz) → AudioWorkletNode 持续 post Float32 块
 * 到主线程累积进增长数组；并行 AnalyserNode 算实时音量。
 * 暴露「取当前段 PCM16 base64 / 清空段 / 读音量 / 停止」。
 */

const TARGET_SAMPLE_RATE = 16000 as const
const WORKLET_URL = '/stt/pcm-worklet.js'

export interface StreamingCapture {
  /** 开始采集。deviceId 为 null 用系统默认麦克风。 */
  start: (deviceId: string | null) => Promise<void>
  /** 停止采集，释放所有资源。 */
  stop: () => void
  /** 当前段累积 PCM 的 PCM16LE base64（喂 stt_transcribe）。空段返回 ''。 */
  getSegmentPcmBase64: () => string
  /** 清空当前段累积 buffer（段定稿后调）。 */
  resetSegment: () => void
  /** 0..1 实时峰值音量。 */
  getVolume: () => number
}

export async function createStreamingCapture(): Promise<StreamingCapture> {
  let stream: MediaStream | null = null
  let audioContext: AudioContext | null = null
  let workletNode: AudioWorkletNode | null = null
  let analyser: AnalyserNode | null = null
  let volumeBuf: Uint8Array<ArrayBuffer> | null = null
  // 当前段累积的 Float32 块。
  let segmentChunks: Float32Array[] = []

  const start = async (deviceId: string | null): Promise<void> => {
    const constraints: MediaStreamConstraints = {
      audio: deviceId ? { deviceId: { exact: deviceId } } : true,
      video: false,
    }
    stream = await navigator.mediaDevices.getUserMedia(constraints)

    audioContext = new (window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)({
      sampleRate: TARGET_SAMPLE_RATE,
    })
    await audioContext.audioWorklet.addModule(WORKLET_URL)

    const source = audioContext.createMediaStreamSource(stream)

    workletNode = new AudioWorkletNode(audioContext, 'pcm-worklet')
    workletNode.port.onmessage = (e: MessageEvent) => {
      const pcm = e.data as Float32Array
      if (pcm && pcm.length > 0) segmentChunks.push(pcm)
    }
    source.connect(workletNode)

    analyser = audioContext.createAnalyser()
    analyser.fftSize = 256
    source.connect(analyser)
    volumeBuf = new Uint8Array(analyser.frequencyBinCount)
  }

  const stop = (): void => {
    try {
      workletNode?.disconnect()
    } catch {
      // ignore
    }
    if (workletNode) workletNode.port.onmessage = null
    stream?.getTracks().forEach((t) => {
      try {
        t.stop()
      } catch {
        // ignore
      }
    })
    audioContext?.close().catch(() => {
      // ignore
    })
    stream = null
    audioContext = null
    workletNode = null
    analyser = null
    volumeBuf = null
    segmentChunks = []
  }

  const getSegmentPcmBase64 = (): string => {
    const total = segmentChunks.reduce((sum, c) => sum + c.length, 0)
    if (total === 0) return ''
    // 合并所有块 → Int16 PCM little-endian → base64。
    const pcm = new Int16Array(total)
    let offset = 0
    for (const chunk of segmentChunks) {
      for (let i = 0; i < chunk.length; i++) {
        const s = Math.max(-1, Math.min(1, chunk[i]!))
        pcm[offset++] = s < 0 ? Math.round(s * 0x8000) : Math.round(s * 0x7fff)
      }
    }
    const bytes = new Uint8Array(pcm.buffer)
    const CHUNK = 0x8000
    let str = ''
    for (let i = 0; i < bytes.length; i += CHUNK) {
      const end = Math.min(i + CHUNK, bytes.length)
      const chars: number[] = []
      for (let j = i; j < end; j++) {
        chars.push(bytes[j]!)
      }
      str += String.fromCharCode(...chars)
    }
    return btoa(str)
  }

  const resetSegment = (): void => {
    segmentChunks = []
  }

  const getVolume = (): number => {
    if (!analyser || !volumeBuf) return 0
    analyser.getByteFrequencyData(volumeBuf)
    let sum = 0
    for (let i = 0; i < volumeBuf.length; i++) sum += volumeBuf[i]!
    const avg = sum / volumeBuf.length
    return Math.max(0, Math.min(1, avg / 255))
  }

  return { start, stop, getSegmentPcmBase64, resetSegment, getVolume }
}

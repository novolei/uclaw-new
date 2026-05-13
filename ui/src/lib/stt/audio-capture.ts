/**
 * audio-capture — browser-side recording for STT.
 *
 * Captures via MediaRecorder, runs a parallel AudioContext + AnalyserNode for
 * real-time volume readings (drives the 5-bar waveform in InlineRecorder),
 * and converts the recorded blob to PCM16LE @ 16kHz base64 for the backend.
 */

export interface StartRecordingOptions {
  /** `null` → default device; otherwise the deviceId from navigator.mediaDevices.enumerateDevices(). */
  deviceId: string | null
}

export interface ActiveRecording {
  stream: MediaStream
  mediaRecorder: MediaRecorder
  audioContext: AudioContext
  analyser: AnalyserNode
  chunks: Blob[]
  startedAtMs: number
  /** Returns a normalized volume in 0..1 sampled from the analyser. */
  readVolume: () => number
}

export interface EncodedAudio {
  /** PCM16LE bytes, base64-encoded. */
  audioBytesBase64: string
  /** Always 16000 (we resample in decodeAudioData via AudioContext sample rate). */
  sampleRate: 16000
}

const TARGET_SAMPLE_RATE = 16000 as const

export async function startRecording(opts: StartRecordingOptions): Promise<ActiveRecording> {
  const constraints: MediaStreamConstraints = {
    audio: opts.deviceId ? { deviceId: { exact: opts.deviceId } } : true,
    video: false,
  }
  const stream = await navigator.mediaDevices.getUserMedia(constraints)

  // Prefer audio/webm; fall back to whatever the UA supports.
  const mimeCandidates = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg']
  const mimeType =
    mimeCandidates.find((m) =>
      typeof (MediaRecorder as { isTypeSupported?: (s: string) => boolean }).isTypeSupported === 'function'
        ? (MediaRecorder as { isTypeSupported: (s: string) => boolean }).isTypeSupported(m)
        : false,
    ) ?? ''

  const mediaRecorder = mimeType
    ? new MediaRecorder(stream, { mimeType })
    : new MediaRecorder(stream)
  const chunks: Blob[] = []
  mediaRecorder.addEventListener('dataavailable', (e: BlobEvent) => {
    if (e.data && e.data.size > 0) chunks.push(e.data)
  })

  // Use a context with the target sample rate so decodeAudioData later
  // produces 16kHz samples directly (no manual resampling needed).
  const audioContext: AudioContext = new (window.AudioContext ||
    (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)({
    sampleRate: TARGET_SAMPLE_RATE,
  })
  const source = audioContext.createMediaStreamSource(stream)
  const analyser = audioContext.createAnalyser()
  analyser.fftSize = 256
  source.connect(analyser)

  const buf = new Uint8Array(analyser.frequencyBinCount)
  const readVolume = (): number => {
    analyser.getByteFrequencyData(buf)
    let sum = 0
    for (let i = 0; i < buf.length; i++) sum += buf[i]!
    const avg = sum / buf.length
    return Math.max(0, Math.min(1, avg / 255))
  }

  mediaRecorder.start(100) // emit dataavailable every 100ms for live volume even before stop

  return {
    stream,
    mediaRecorder,
    audioContext,
    analyser,
    chunks,
    startedAtMs: Date.now(),
    readVolume,
  }
}

export async function stopAndEncode(rec: ActiveRecording): Promise<EncodedAudio> {
  // Wait for the final 'stop' event so all chunks land in `rec.chunks`.
  await new Promise<void>((resolve) => {
    rec.mediaRecorder.addEventListener('stop', () => resolve(), { once: true })
    if (rec.mediaRecorder.state !== 'inactive') rec.mediaRecorder.stop()
    else resolve()
  })

  rec.stream.getTracks().forEach((t) => t.stop())

  const blob = new Blob(rec.chunks, { type: rec.mediaRecorder.mimeType || 'audio/webm' })
  const arrayBuf = await blobToArrayBuffer(blob)

  // Decode → Float32 mono @ 16kHz (AudioContext was constructed with target SR).
  const audioBuf = await rec.audioContext.decodeAudioData(arrayBuf.slice(0))
  const channelData = audioBuf.getChannelData(0)

  // Float32 [-1, 1] → Int16 PCM little-endian.
  const pcm = new Int16Array(channelData.length)
  for (let i = 0; i < channelData.length; i++) {
    const s = Math.max(-1, Math.min(1, channelData[i]!))
    pcm[i] = s < 0 ? Math.round(s * 0x8000) : Math.round(s * 0x7fff)
  }
  const bytes = new Uint8Array(pcm.buffer)

  // Base64 encode (chunked to avoid call-stack overflow on >100kB).
  const audioBytesBase64 = bytesToBase64(bytes)

  await rec.audioContext.close()

  return {
    audioBytesBase64,
    sampleRate: TARGET_SAMPLE_RATE,
  }
}

export function cancelRecording(rec: ActiveRecording): void {
  try {
    if (rec.mediaRecorder.state !== 'inactive') rec.mediaRecorder.stop()
  } catch {
    // ignore
  }
  rec.stream.getTracks().forEach((t) => {
    try {
      t.stop()
    } catch {
      // ignore
    }
  })
  rec.audioContext.close().catch(() => {
    // ignore
  })
}

/** Reads a Blob as an ArrayBuffer, with FileReader fallback for jsdom compatibility. */
function blobToArrayBuffer(blob: Blob): Promise<ArrayBuffer> {
  if (typeof blob.arrayBuffer === 'function') {
    return blob.arrayBuffer()
  }
  // FileReader fallback (jsdom < v26 doesn't expose Blob.prototype.arrayBuffer)
  return new Promise<ArrayBuffer>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as ArrayBuffer)
    reader.onerror = () => reject(reader.error)
    reader.readAsArrayBuffer(blob)
  })
}

function bytesToBase64(bytes: Uint8Array): string {
  const CHUNK = 0x8000
  let str = ''
  for (let i = 0; i < bytes.length; i += CHUNK) {
    const slice = bytes.subarray(i, i + CHUNK)
    str += String.fromCharCode(...slice)
  }
  return btoa(str)
}

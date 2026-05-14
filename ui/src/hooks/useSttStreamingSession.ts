/**
 * useSttStreamingSession — 流式语音 modal 的会话编排 FSM。
 *
 * 替代 useSttRecording。驱动 SttModal：claim 跨 composer 锁、启停 streaming-capture、
 * 段内每 ~1.5s 重转写做实时预览（Task 5）、静音超时把段定稿并通过 onSegmentFinalized
 * 回调追加到聊天输入框（Task 6）。
 */
import { useCallback, useEffect, useRef } from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import {
  sttModalStateAtom,
  activeComposerAtom,
  modelStatusAtom,
  sttSettingsAtom,
  type ComposerKind,
  type SttModalState,
} from '@/atoms/stt-atoms'
import { createStreamingCapture, type StreamingCapture } from '@/lib/stt/streaming-capture'
import { regularizePunctuation } from '@/lib/stt/punctuation'

export const RETRANSCRIBE_INTERVAL_MS = 1500
export const VOLUME_SAMPLE_INTERVAL_MS = 80
export const MIN_SEGMENT_MS = 800
/** 音量高于此值视为「有人在说话」。 */
export const VOICE_VOLUME_THRESHOLD = 0.04
/** EQ 音量条的段数。 */
export const BAR_COUNT = 7

export type StartResult = 'started' | 'busy' | 'needs-download' | 'permission-denied' | 'error'

interface UseSttStreamingSessionOptions {
  /** 每段定稿后调用，参数是规整过标点的文本。由调用方追加到聊天输入框。 */
  onSegmentFinalized?: (text: string) => void
}

export interface SttSessionHandle {
  state: SttModalState
  start: () => Promise<StartResult>
  /** 主动结束：定稿当前未完成段（若有），然后关闭。 */
  end: () => Promise<void>
  /** 取消：丢弃当前段，直接关闭。 */
  cancel: () => void
}

export function useSttStreamingSession(
  composer: ComposerKind,
  opts: UseSttStreamingSessionOptions = {},
): SttSessionHandle {
  const [sharedState, setState] = useAtom(sttModalStateAtom)
  const [active, setActive] = useAtom(activeComposerAtom)
  const state: SttModalState =
    active === composer || active === null ? sharedState : { kind: 'idle' }
  const modelStatus = useAtomValue(modelStatusAtom)
  const settings = useAtomValue(sttSettingsAtom)

  const captureRef = useRef<StreamingCapture | null>(null)
  const retranscribeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const volumeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const transcribeInFlightRef = useRef(false)
  const lastVoiceMsRef = useRef(0)
  const segmentStartedMsRef = useRef(0)
  const interimTextRef = useRef('')
  const endingRef = useRef(false)
  /** 静音定稿进行中的锁 — 防止同一段被多次定稿。 */
  const silenceFinalizeInProgressRef = useRef(false)
  // opts.onSegmentFinalized 用 ref 持有，避免 effect/interval 闭包过期。
  const onSegmentFinalizedRef = useRef(opts.onSegmentFinalized)
  onSegmentFinalizedRef.current = opts.onSegmentFinalized
  // active 的当前值用 ref 持有，供卸载 cleanup 闭包读取最新值。
  const activeRef = useRef(active)
  activeRef.current = active

  const clearTimers = useCallback(() => {
    if (retranscribeTimerRef.current) {
      clearInterval(retranscribeTimerRef.current)
      retranscribeTimerRef.current = null
    }
    if (volumeTimerRef.current) {
      clearInterval(volumeTimerRef.current)
      volumeTimerRef.current = null
    }
  }, [])

  const teardown = useCallback(() => {
    clearTimers()
    captureRef.current?.stop()
    captureRef.current = null
    transcribeInFlightRef.current = false
    interimTextRef.current = ''
    endingRef.current = false
    silenceFinalizeInProgressRef.current = false
  }, [clearTimers])

  // 转写当前段一次，返回原始文本（不加标点）。失败抛错。
  const transcribeSegment = useCallback(async (): Promise<string> => {
    const cap = captureRef.current
    if (!cap) return ''
    const audio = cap.getSegmentPcmBase64()
    if (audio === '') return ''
    const result = (await invoke('stt_transcribe', {
      request: {
        audio_bytes_base64: audio,
        language: settings.language === 'auto' ? null : settings.language,
        sample_rate: 16000,
      },
    })) as { text: string }
    return result.text
  }, [settings.language])

  // ── Task 5 会填充：startRetranscribeLoop ─────────────────────────
  const startRetranscribeLoop = useCallback(() => {
    retranscribeTimerRef.current = setInterval(() => {
      // in-flight 守卫：上一次转写没返回就跳过这一拍，避免请求堆积。
      if (transcribeInFlightRef.current) return
      const cap = captureRef.current
      if (!cap) return
      transcribeInFlightRef.current = true
      void transcribeSegment()
        .then((raw) => {
          interimTextRef.current = raw
          setState((prev) =>
            prev.kind === 'listening' ? { ...prev, interimText: raw } : prev,
          )
        })
        .catch(() => {
          // 段内 tick 失败：跳过这拍，不打断会话。
        })
        .finally(() => {
          transcribeInFlightRef.current = false
        })
    }, RETRANSCRIBE_INTERVAL_MS)
  }, [setState, transcribeSegment])

  // ── Task 6 会填充：finalizeSegment + 静音检测 ────────────────────
  const finalizeSegment = useCallback(async (): Promise<void> => {
    // Task 6 实现：转写当前段 → regularizePunctuation → onSegmentFinalized
    // → resetSegment → 重置 interim/segment 计时 → 回 listening
    const cap = captureRef.current
    if (!cap) return
    try {
      const raw = await transcribeSegment()
      const text = regularizePunctuation(raw, settings.language)
      if (text) onSegmentFinalizedRef.current?.(text)
    } finally {
      cap.resetSegment()
      interimTextRef.current = ''
      segmentStartedMsRef.current = Date.now()
      lastVoiceMsRef.current = Date.now()
    }
  }, [transcribeSegment, settings.language])

  const startVolumeLoop = useCallback(() => {
    volumeTimerRef.current = setInterval(() => {
      const cap = captureRef.current
      if (!cap) return
      const v = cap.getVolume()
      const bands = cap.getFrequencyBands(BAR_COUNT)
      const now = Date.now()
      if (v > VOICE_VOLUME_THRESHOLD) lastVoiceMsRef.current = now

      // 静音定稿判定：静音够久 + 当前段有内容 + 段时长够（防气音误触发）。
      const silentFor = now - lastVoiceMsRef.current
      const segmentAge = now - segmentStartedMsRef.current
      const shouldFinalize =
        silentFor > (settings.silenceThresholdMs ?? 1800) &&
        interimTextRef.current.trim() !== '' &&
        segmentAge > MIN_SEGMENT_MS &&
        // 不在重转写途中才定稿——保证定稿启动时没有 in-flight 的段内转写，
        // 配合下面的 clearInterval，定稿期间不会有「旧段重转写结果」回写串台。
        !transcribeInFlightRef.current &&
        !silenceFinalizeInProgressRef.current &&
        !endingRef.current

      if (shouldFinalize) {
        // 进 finalizing：暂停重转写循环，定稿，再恢复。
        silenceFinalizeInProgressRef.current = true
        if (retranscribeTimerRef.current) {
          clearInterval(retranscribeTimerRef.current)
          retranscribeTimerRef.current = null
        }
        setState({ kind: 'finalizing', volume: v })
        void finalizeSegment()
          .catch(() => {
            // 定稿失败：进 error 态 1500ms 后关闭，已定稿的段不丢。
            silenceFinalizeInProgressRef.current = false
            teardown()
            setActive(null)
            setState({ kind: 'error', message: '转写失败' })
            setTimeout(() => setState({ kind: 'idle' }), 1500)
          })
          .then(() => {
            // 成功：回 listening，重启重转写循环。
            silenceFinalizeInProgressRef.current = false
            if (captureRef.current && !endingRef.current) {
              setState({
                kind: 'listening',
                segmentStartedMs: segmentStartedMsRef.current,
                volume: 0,
                bands: [],
                interimText: '',
              })
              startRetranscribeLoop()
            }
          })
        return
      }

      // 普通帧：刷新音量 + EQ 频段。
      setState((prev) =>
        prev.kind === 'listening' ? { ...prev, volume: v, bands } : prev,
      )
    }, VOLUME_SAMPLE_INTERVAL_MS)
  }, [
    settings.silenceThresholdMs,
    finalizeSegment,
    startRetranscribeLoop,
    setState,
    setActive,
    teardown,
  ])

  const start = useCallback(async (): Promise<StartResult> => {
    if (active !== null && active !== composer) return 'busy'
    if (modelStatus.kind !== 'ready') return 'needs-download'
    setActive(composer)
    setState({ kind: 'requesting-permission' })
    let cap: StreamingCapture
    try {
      cap = await createStreamingCapture()
      await cap.start(settings.microphoneDeviceId)
    } catch (e) {
      setActive(null)
      const name = (e as { name?: string })?.name
      if (name === 'NotAllowedError' || name === 'SecurityError') {
        setState({ kind: 'permission-denied' })
        return 'permission-denied'
      }
      setState({ kind: 'error', message: String((e as Error)?.message ?? e) })
      setTimeout(() => setState({ kind: 'idle' }), 1500)
      return 'error'
    }
    captureRef.current = cap
    const now = Date.now()
    segmentStartedMsRef.current = now
    lastVoiceMsRef.current = now
    interimTextRef.current = ''
    endingRef.current = false
    setState({ kind: 'listening', segmentStartedMs: now, volume: 0, bands: [], interimText: '' })
    startRetranscribeLoop()
    startVolumeLoop()
    return 'started'
  }, [
    active,
    composer,
    modelStatus.kind,
    settings.microphoneDeviceId,
    setActive,
    setState,
    startRetranscribeLoop,
    startVolumeLoop,
  ])

  const end = useCallback(async (): Promise<void> => {
    if (endingRef.current) return
    endingRef.current = true
    clearTimers()
    // 若当前段有内容、且静音定稿没在进行中，先自己定稿一次。
    // 静音定稿进行中时跳过——那个 in-flight 的 finalizeSegment 会负责这段，
    // 否则 end() 会对同一段再定稿一次，导致文本被追加两遍。
    if (!silenceFinalizeInProgressRef.current && interimTextRef.current.trim() !== '') {
      setState({ kind: 'finalizing', volume: 0 })
      try {
        await finalizeSegment()
      } catch {
        // 定稿失败也照常关闭，已定稿的段不丢。
      }
    }
    teardown()
    setActive(null)
    setState({ kind: 'idle' })
  }, [clearTimers, finalizeSegment, setActive, setState, teardown])

  const cancel = useCallback(() => {
    teardown()
    setActive(null)
    setState({ kind: 'idle' })
  }, [setActive, setState, teardown])

  // 卸载时清理。
  useEffect(() => {
    return () => {
      clearTimers()
      captureRef.current?.stop()
      captureRef.current = null
      // 若本 composer 持有当前会话，卸载时一并复位共享原子，
      // 否则切到另一个 composer 时它的 SttModal 会读到残留状态、渲染出幽灵 modal。
      if (activeRef.current === composer) {
        setActive(null)
        setState({ kind: 'idle' })
      }
    }
  }, [clearTimers, composer, setActive, setState])

  return { state, start, end, cancel }
}

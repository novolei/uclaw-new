/**
 * useMemoryVoiceSession — 语音记忆录制的会话编排 FSM Hook。
 *
 * 与 useSttStreamingSession（语音输入）完全独立。驱动 memoryVoiceStateAtom，
 * 复用底层 streaming-capture 和 stt_transcribe，但采用单段模式：
 * 静音定稿 → 保存到 Memory Graph → 自动关闭。
 */
import { useCallback, useEffect, useRef } from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import { memoryVoiceStateAtom, type MemoryVoiceState } from '@/atoms/memory-voice-atoms'
import { sttModalStateAtom, modelStatusAtom, sttSettingsAtom } from '@/atoms/stt-atoms'
import { createStreamingCapture, type StreamingCapture } from '@/lib/stt/streaming-capture'
import { regularizePunctuation } from '@/lib/stt/punctuation'

// ── 参数配置（与 STT 差异化） ──────────────────────────────────────────
const SILENCE_THRESHOLD_MS = 2400      // STT 是 1800，给用户更多思考时间
const MIN_SEGMENT_MS = 1200            // STT 是 800，避免气音/咳嗽误触
const RETRANSCRIBE_INTERVAL_MS = 1500  // 复用 STT 相同值
const VOLUME_SAMPLE_INTERVAL_MS = 80   // 复用
const VOICE_VOLUME_THRESHOLD = 0.04    // 复用
const AUTO_CLOSE_DELAY_MS = 1500       // 保存成功后自动关闭延迟

export interface UseMemoryVoiceSessionReturn {
  /** 启动语音记忆录制 */
  start(): Promise<'ok' | 'needs-download' | 'permission-denied' | 'conflict'>
  /** 取消录制（不保存） */
  cancel(): void
  /** 当前状态（从 atom 读取） */
  state: MemoryVoiceState
  /** 当前音量 0-1 */
  volume: number
}

export function useMemoryVoiceSession(): UseMemoryVoiceSessionReturn {
  const state = useAtomValue(memoryVoiceStateAtom)
  const setState = useSetAtom(memoryVoiceStateAtom)
  const sttState = useAtomValue(sttModalStateAtom)
  const modelStatus = useAtomValue(modelStatusAtom)
  const settings = useAtomValue(sttSettingsAtom)

  // ── Refs ──────────────────────────────────────────────────────────────
  const captureRef = useRef<StreamingCapture | null>(null)
  const retranscribeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const volumeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const transcribeInFlightRef = useRef(false)
  const lastVoiceMsRef = useRef(0)
  const segmentStartedMsRef = useRef(0)
  const interimTextRef = useRef('')
  const volumeRef = useRef(0)
  /** 静音定稿进行中的锁 — 防止同一段被多次定稿。 */
  const silenceFinalizeInProgressRef = useRef(false)

  // ── 定时器清理 ────────────────────────────────────────────────────────
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
    volumeRef.current = 0
    silenceFinalizeInProgressRef.current = false
  }, [clearTimers])

  // ── 转写一次当前段 ────────────────────────────────────────────────────
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

  // ── 保存到 Memory Graph ───────────────────────────────────────────────
  const saveToMemoryGraph = useCallback(async (text: string): Promise<void> => {
    await invoke('memory_graph_quick_capture', {
      input: {
        content: text,
        source: 'voice',
      },
    })
  }, [])

  // ── 定稿 + 保存 ──────────────────────────────────────────────────────
  const finalizeAndSave = useCallback(async (): Promise<void> => {
    const cap = captureRef.current
    if (!cap) return

    // 最终转写
    let raw: string
    try {
      raw = await transcribeSegment()
    } catch {
      throw new Error('转写失败')
    }

    const text = regularizePunctuation(raw, settings.language)
    if (!text) {
      // 空内容，静默回 idle
      teardown()
      setState({ kind: 'idle' })
      return
    }

    // 进入 saving 状态
    setState({ kind: 'saving', text })
    teardown()

    // 保存到 Memory Graph
    try {
      await saveToMemoryGraph(text)
      // 保存成功 → saved 状态
      setState({ kind: 'saved', title: text.slice(0, 20), subtype: 'voice' })
      // 自动关闭延迟
      setTimeout(() => setState({ kind: 'idle' }), AUTO_CLOSE_DELAY_MS)
    } catch {
      // 保存失败：将文本复制到系统剪贴板
      try {
        await navigator.clipboard.writeText(text)
      } catch {
        // 剪贴板写入也失败，无法挽救
      }
      setState({ kind: 'error', message: '保存失败，已复制到剪贴板' })
      setTimeout(() => setState({ kind: 'idle' }), 2000)
    }
  }, [transcribeSegment, settings.language, teardown, setState, saveToMemoryGraph])

  // ── 重转写循环（段内实时预览） ────────────────────────────────────────
  const startRetranscribeLoop = useCallback(() => {
    retranscribeTimerRef.current = setInterval(() => {
      if (transcribeInFlightRef.current) return
      const cap = captureRef.current
      if (!cap) return
      transcribeInFlightRef.current = true
      void transcribeSegment()
        .then((raw) => {
          interimTextRef.current = raw
          setState((prev) =>
            prev.kind === 'recording' ? { ...prev, interimText: raw } : prev,
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

  // ── 音量循环 + 静音定稿检测 ───────────────────────────────────────────
  const startVolumeLoop = useCallback(() => {
    volumeTimerRef.current = setInterval(() => {
      const cap = captureRef.current
      if (!cap) return
      const v = cap.getVolume()
      const now = Date.now()
      if (v > VOICE_VOLUME_THRESHOLD) lastVoiceMsRef.current = now
      volumeRef.current = v

      // 静音定稿判定
      const silentFor = now - lastVoiceMsRef.current
      const segmentAge = now - segmentStartedMsRef.current
      const shouldFinalize =
        silentFor > SILENCE_THRESHOLD_MS &&
        interimTextRef.current.trim() !== '' &&
        segmentAge > MIN_SEGMENT_MS &&
        !transcribeInFlightRef.current &&
        !silenceFinalizeInProgressRef.current

      if (shouldFinalize) {
        // 单段模式：触发定稿后直接保存并关闭
        silenceFinalizeInProgressRef.current = true
        if (retranscribeTimerRef.current) {
          clearInterval(retranscribeTimerRef.current)
          retranscribeTimerRef.current = null
        }
        void finalizeAndSave().catch(() => {
          // finalizeAndSave 内部已处理错误状态
        })
        return
      }

      // 普通帧：刷新音量
      setState((prev) =>
        prev.kind === 'recording' ? { ...prev, volume: v } : prev,
      )
    }, VOLUME_SAMPLE_INTERVAL_MS)
  }, [finalizeAndSave, setState])

  // ── start() ───────────────────────────────────────────────────────────
  const start = useCallback(async (): Promise<'ok' | 'needs-download' | 'permission-denied' | 'conflict'> => {
    // 互斥检查：STT 正在使用
    if (sttState.kind !== 'idle') return 'conflict'
    // 模型检查
    if (modelStatus.kind !== 'ready') return 'needs-download'

    setState({ kind: 'requesting-permission' })

    let cap: StreamingCapture
    try {
      cap = await createStreamingCapture()
      await cap.start(settings.microphoneDeviceId)
    } catch (e) {
      const name = (e as { name?: string })?.name
      if (name === 'NotAllowedError' || name === 'SecurityError') {
        setState({ kind: 'permission-denied' })
        setTimeout(() => setState({ kind: 'idle' }), 2000)
        return 'permission-denied'
      }
      setState({ kind: 'error', message: String((e as Error)?.message ?? e) })
      setTimeout(() => setState({ kind: 'idle' }), 2000)
      return 'permission-denied'
    }

    captureRef.current = cap
    const now = Date.now()
    segmentStartedMsRef.current = now
    lastVoiceMsRef.current = now
    interimTextRef.current = ''
    volumeRef.current = 0
    silenceFinalizeInProgressRef.current = false
    setState({ kind: 'recording', interimText: '', volume: 0 })
    startRetranscribeLoop()
    startVolumeLoop()
    return 'ok'
  }, [
    sttState.kind,
    modelStatus.kind,
    settings.microphoneDeviceId,
    setState,
    startRetranscribeLoop,
    startVolumeLoop,
  ])

  // ── cancel() ──────────────────────────────────────────────────────────
  const cancel = useCallback(() => {
    teardown()
    setState({ kind: 'idle' })
  }, [teardown, setState])

  // ── 卸载清理 ──────────────────────────────────────────────────────────
  useEffect(() => {
    return () => {
      clearTimers()
      captureRef.current?.stop()
      captureRef.current = null
    }
  }, [clearTimers])

  // 导出 volume（从 state 中取或 ref）
  const volume = state.kind === 'recording' ? state.volume : 0

  return { start, cancel, state, volume }
}

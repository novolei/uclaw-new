/**
 * useSttRecording — finite state machine wrapping audio-capture + Tauri stt_transcribe.
 *
 * Drives the inline recorder UI on both ChatInput and AgentView. Coordinates a
 * single global recording session via `activeComposerAtom` so only one composer
 * can record at a time. Auto-stops at 60s.
 */
import { useCallback, useEffect, useRef } from 'react'
import { useAtom } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import {
  recordingStateAtom,
  activeComposerAtom,
  modelStatusAtom,
  sttSettingsAtom,
  type ComposerKind,
  type RecordingState,
} from '@/atoms/stt-atoms'
import {
  startRecording,
  stopAndEncode,
  cancelRecording,
  type ActiveRecording,
} from '@/lib/stt/audio-capture'

export const MAX_RECORDING_MS = 60_000
export const WARNING_AFTER_MS = 50_000

interface UseSttRecordingOptions {
  /** Called with the final transcript text after a successful stop(). */
  onTranscribe?: (text: string) => void
}

export type StartResult = 'started' | 'busy' | 'needs-download' | 'permission-denied' | 'error'

interface SttHandle {
  state: RecordingState
  start: () => Promise<StartResult>
  stop: () => Promise<void>
  cancel: () => void
}

export function useSttRecording(
  composer: ComposerKind,
  opts: UseSttRecordingOptions = {},
): SttHandle {
  const [sharedState, setState] = useAtom(recordingStateAtom)
  const [active, setActive] = useAtom(activeComposerAtom)
  // Each composer only "owns" the shared state when it is the active one.
  // If another composer is recording, this composer reports idle to its callers.
  const state: RecordingState = active === composer || active === null ? sharedState : { kind: 'idle' }
  const [modelStatus] = useAtom(modelStatusAtom)
  const [settings] = useAtom(sttSettingsAtom)
  const activeRef = useRef<ActiveRecording | null>(null)
  const autoStopTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearAutoStop = useCallback(() => {
    if (autoStopTimerRef.current) {
      clearTimeout(autoStopTimerRef.current)
      autoStopTimerRef.current = null
    }
  }, [])

  const stop = useCallback(async () => {
    clearAutoStop()
    const rec = activeRef.current
    if (!rec || state.kind !== 'recording') return
    setState({ kind: 'transcribing' })
    try {
      const encoded = await stopAndEncode(rec)
      activeRef.current = null
      const result = (await invoke('stt_transcribe', {
        request: {
          audio_bytes_base64: encoded.audioBytesBase64,
          language: settings.language === 'auto' ? null : settings.language,
          sample_rate: encoded.sampleRate,
        },
      })) as { text: string }
      setState({ kind: 'done', text: result.text })
      opts.onTranscribe?.(result.text)
      // Flash 'done' for 300ms then return to idle.
      setTimeout(() => setState({ kind: 'idle' }), 300)
    } catch (e) {
      setState({ kind: 'error', message: String((e as Error)?.message ?? e) })
      setTimeout(() => setState({ kind: 'idle' }), 1500)
    } finally {
      setActive(null)
    }
  }, [clearAutoStop, opts, settings.language, setActive, setState, state.kind])

  const start = useCallback(async (): Promise<StartResult> => {
    if (active !== null && active !== composer) return 'busy'
    if (modelStatus.kind !== 'ready') return 'needs-download'
    setActive(composer)
    setState({ kind: 'requesting-permission' })
    let rec: ActiveRecording
    try {
      rec = await startRecording({ deviceId: settings.microphoneDeviceId })
    } catch (e) {
      setActive(null)
      const name = (e as { name?: string })?.name
      if (name === 'NotAllowedError' || name === 'SecurityError') {
        setState({ kind: 'permission-denied' })
        return 'permission-denied'
      }
      setState({
        kind: 'error',
        message: String((e as Error)?.message ?? e),
      })
      setTimeout(() => setState({ kind: 'idle' }), 1500)
      return 'error'
    }
    activeRef.current = rec
    setState({ kind: 'recording', startedAtMs: rec.startedAtMs, volume: 0 })
    autoStopTimerRef.current = setTimeout(() => {
      void stop()
    }, MAX_RECORDING_MS)
    return 'started'
  }, [active, composer, modelStatus.kind, setActive, setState, settings.microphoneDeviceId, stop])

  const cancel = useCallback(() => {
    clearAutoStop()
    const rec = activeRef.current
    if (rec) {
      cancelRecording(rec)
      activeRef.current = null
    }
    setActive(null)
    setState({ kind: 'idle' })
  }, [clearAutoStop, setActive, setState])

  // Pump live volume into the recording state for the waveform UI.
  useEffect(() => {
    if (state.kind !== 'recording') return
    const id = setInterval(() => {
      const rec = activeRef.current
      if (!rec) return
      const v = rec.readVolume()
      setState((prev) =>
        prev.kind === 'recording' ? { ...prev, volume: v } : prev,
      )
    }, 80)
    return () => clearInterval(id)
  }, [state.kind, setState])

  useEffect(() => {
    return () => {
      clearAutoStop()
      const rec = activeRef.current
      if (rec) cancelRecording(rec)
    }
  }, [clearAutoStop])

  return { state, start, stop, cancel }
}

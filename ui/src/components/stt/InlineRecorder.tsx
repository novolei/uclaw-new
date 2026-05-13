/**
 * InlineRecorder — Proma-style inline recording UI for the composer.
 *
 * 5-bar waveform driven by real AnalyserNode volume (passed through
 * recordingState.volume by useSttRecording), mm:ss timer, cancel/stop
 * controls. Renders inline next to SpeechButton when state.kind is one
 * of: recording / transcribing.
 */
import * as React from 'react'
import { motion, AnimatePresence } from 'motion/react'
import { X, Check, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { RecordingState } from '@/atoms/stt-atoms'

interface InlineRecorderProps {
  state: RecordingState
  onStop: () => void
  onCancel: () => void
}

const BAR_HEIGHT_SCALES = [0.6, 1.0, 0.75, 0.9, 0.5] // Proma's pattern
const WARNING_AFTER_MS = 50_000

function formatTimer(elapsedMs: number): string {
  const totalSec = Math.floor(elapsedMs / 1000)
  const m = Math.floor(totalSec / 60)
  const s = totalSec % 60
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

export function InlineRecorder({
  state,
  onStop,
  onCancel,
}: InlineRecorderProps): React.ReactElement | null {
  const [now, setNow] = React.useState(Date.now())
  React.useEffect(() => {
    if (state.kind !== 'recording') return
    const id = setInterval(() => setNow(Date.now()), 200)
    return () => clearInterval(id)
  }, [state.kind])

  if (state.kind !== 'recording' && state.kind !== 'transcribing') return null

  const elapsedMs =
    state.kind === 'recording' ? now - state.startedAtMs : 0
  const warning = elapsedMs >= WARNING_AFTER_MS
  const volume = state.kind === 'recording' ? state.volume : 0

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, width: 0 }}
        animate={{ opacity: 1, width: 'auto' }}
        exit={{ opacity: 0, width: 0 }}
        transition={{ duration: 0.2, ease: 'easeInOut' }}
        className={cn(
          'flex items-center gap-2 px-3 py-1.5 rounded-full',
          'bg-primary/10 border border-primary/30',
          'overflow-hidden',
        )}
        data-testid="stt-inline-recorder"
      >
        {/* Waveform — real-volume driven */}
        <div className="flex items-center gap-[3px] h-4">
          {BAR_HEIGHT_SCALES.map((scale, i) => {
            const h = state.kind === 'recording'
              ? Math.max(4, Math.round(volume * scale * 16))
              : 4
            return (
              <span
                key={i}
                data-testid="stt-waveform-bar"
                className="w-[3px] rounded-full bg-primary transition-all duration-100"
                style={{ height: `${h}px` }}
              />
            )
          })}
        </div>

        {/* Timer or transcribing indicator */}
        {state.kind === 'recording' ? (
          <span
            data-testid="stt-timer"
            className={cn(
              'text-xs font-mono tabular-nums',
              warning ? 'text-amber-600' : 'text-muted-foreground',
            )}
          >
            {formatTimer(elapsedMs)}
          </span>
        ) : (
          <span className="text-xs text-muted-foreground inline-flex items-center gap-1">
            <Loader2 className="size-3 animate-spin" />
            转写中…
          </span>
        )}

        {/* Cancel — drop without transcribing */}
        <button
          type="button"
          aria-label="取消录音"
          onClick={onCancel}
          disabled={state.kind === 'transcribing'}
          className={cn(
            'size-5 rounded-full inline-flex items-center justify-center',
            'text-muted-foreground hover:text-destructive hover:bg-destructive/10',
            'disabled:opacity-40 disabled:cursor-not-allowed transition-colors',
          )}
        >
          <X className="size-3.5" />
        </button>

        {/* Stop + transcribe */}
        <button
          type="button"
          aria-label="完成并转写"
          onClick={onStop}
          disabled={state.kind === 'transcribing'}
          className={cn(
            'size-5 rounded-full inline-flex items-center justify-center',
            'text-primary hover:bg-primary/20',
            'disabled:opacity-40 disabled:cursor-not-allowed transition-colors',
          )}
        >
          <Check className="size-3.5" />
        </button>
      </motion.div>
    </AnimatePresence>
  )
}

/**
 * SpeechButton — toggle for inline voice recording in the chat / agent composer.
 *
 * - Click while `modelStatus.kind !== 'ready'` → opens FirstRunDialog (caller-supplied).
 * - Click while ready & idle → starts recording via useSttRecording.
 * - Click while recording → stops + transcribes (alias for InlineRecorder's check button).
 *
 * The actual InlineRecorder UI is rendered by the parent composer so it sits in
 * the action row between SpeechButton and the next tool.
 *
 * Window events drive integration with InlineRecorder + FirstRunDialog (Task 13/14):
 *   - 'uclaw:stt-stop'              → if recording, stop + transcribe
 *   - 'uclaw:stt-cancel'            → if recording, drop audio
 *   - 'uclaw:stt-start-after-ready' → if idle, start recording (fired by FirstRunDialog when download completes)
 */
import * as React from 'react'
import { Mic, MicOff } from 'lucide-react'
import { useAtomValue } from 'jotai'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { useSttRecording } from '@/hooks/useSttRecording'
import { useShortcut } from '@/hooks/useShortcut'
import { modelStatusAtom, type ComposerKind } from '@/atoms/stt-atoms'

interface SpeechButtonProps {
  composer?: ComposerKind
  onTranscript: (text: string) => void
  /** Called when the user clicks the mic and the model isn't downloaded yet. */
  onShowDownloadDialog?: () => void
  /** Optional callback after a successful transcript — used by parent to trigger auto-send. */
  onAfterTranscribe?: (text: string) => void
}

export function SpeechButton({
  composer = 'chat',
  onTranscript,
  onShowDownloadDialog,
  onAfterTranscribe,
}: SpeechButtonProps): React.ReactElement {
  const modelStatus = useAtomValue(modelStatusAtom)
  const handleTranscript = React.useCallback(
    (text: string) => {
      onTranscript(text)
      onAfterTranscribe?.(text)
    },
    [onTranscript, onAfterTranscribe],
  )
  const stt = useSttRecording(composer, { onTranscribe: handleTranscript })

  const handleClick = React.useCallback(async () => {
    if (stt.state.kind === 'recording') {
      await stt.stop()
      return
    }
    if (stt.state.kind === 'transcribing') return
    if (modelStatus.kind !== 'ready') {
      onShowDownloadDialog?.()
      return
    }
    await stt.start()
  }, [modelStatus.kind, onShowDownloadDialog, stt])

  // Global shortcut → only the chat-side instance responds, to avoid double-fire
  // when both ChatInput and AgentView mount their SpeechButton.
  useShortcut({
    id: 'toggle-stt-recording',
    handler: () => {
      void handleClick()
    },
    disabled: composer !== 'chat',
  })

  // Window-event bus for InlineRecorder + FirstRunDialog wiring (Task 14).
  React.useEffect(() => {
    const handleStop = () => {
      if (stt.state.kind === 'recording') void stt.stop()
    }
    const handleCancel = () => {
      if (stt.state.kind === 'recording') stt.cancel()
    }
    const handleStartAfterReady = () => {
      if (stt.state.kind === 'idle') void stt.start()
    }
    window.addEventListener('uclaw:stt-stop', handleStop)
    window.addEventListener('uclaw:stt-cancel', handleCancel)
    window.addEventListener('uclaw:stt-start-after-ready', handleStartAfterReady)
    return () => {
      window.removeEventListener('uclaw:stt-stop', handleStop)
      window.removeEventListener('uclaw:stt-cancel', handleCancel)
      window.removeEventListener('uclaw:stt-start-after-ready', handleStartAfterReady)
    }
  }, [stt])

  const recording =
    stt.state.kind === 'recording' || stt.state.kind === 'transcribing'

  const tooltipText =
    modelStatus.kind === 'ready'
      ? recording
        ? '点击停止录音'
        : '语音输入'
      : modelStatus.kind === 'downloading'
        ? `模型下载中… ${modelStatus.percent}%`
        : '语音输入（点击下载模型）'

  const Icon = stt.state.kind === 'permission-denied' ? MicOff : Mic

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="语音输入"
          onClick={handleClick}
          className={cn(
            'size-[30px] rounded-full transition-colors relative',
            recording
              ? 'text-primary bg-primary/10 hover:bg-primary/20'
              : 'text-foreground/60 hover:text-foreground',
          )}
        >
          <Icon className="size-5" />
          {/* "半启用" 小点：未下载且非录音中时显示 */}
          {modelStatus.kind === 'not-downloaded' && !recording && (
            <span
              aria-hidden
              className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-primary"
            />
          )}
          {/* 下载中：右下角百分比角标 */}
          {modelStatus.kind === 'downloading' && (
            <span
              aria-hidden
              className="absolute -bottom-1 -right-1 text-[8px] font-mono bg-primary text-primary-foreground rounded-full px-1 leading-tight"
            >
              {modelStatus.percent}%
            </span>
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p>{tooltipText}</p>
      </TooltipContent>
    </Tooltip>
  )
}

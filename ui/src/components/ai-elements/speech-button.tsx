/**
 * SpeechButton — 聊天 / agent composer 里的语音输入开关按钮。
 *
 * 点击 / 快捷键 → 开或关流式语音 modal（SttModal）。SpeechButton 本身不持有
 * 录音会话——会话由 SttModal 内部的 useSttStreamingSession 持有。两者通过
 * window 事件桥接：
 *   - 'uclaw:stt-start' → SttModal 调 session.start()
 *   - 'uclaw:stt-end'   → SttModal 调 session.end()
 *   - 'uclaw:stt-start-after-ready' → 模型下载完后由 FirstRunDialog 派发
 */
import * as React from 'react'
import { Mic, MicOff } from 'lucide-react'
import { useAtomValue } from 'jotai'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { useShortcut } from '@/hooks/useShortcut'
import {
  modelStatusAtom,
  sttModalStateAtom,
  activeComposerAtom,
  type ComposerKind,
} from '@/atoms/stt-atoms'

interface SpeechButtonProps {
  composer?: ComposerKind
  /** 点击麦克风但模型还没下载时调用。 */
  onShowDownloadDialog?: () => void
}

export function SpeechButton({
  composer = 'chat',
  onShowDownloadDialog,
}: SpeechButtonProps): React.ReactElement {
  const modelStatus = useAtomValue(modelStatusAtom)
  const modalState = useAtomValue(sttModalStateAtom)
  const activeComposer = useAtomValue(activeComposerAtom)

  // 本 composer 的 modal 是否开着。
  const isOpenHere = modalState.kind !== 'idle' && activeComposer === composer
  // 别的 composer 占用中。
  const isBusyElsewhere =
    modalState.kind !== 'idle' && activeComposer !== null && activeComposer !== composer

  const handleClick = React.useCallback(() => {
    if (isBusyElsewhere) return
    if (isOpenHere) {
      window.dispatchEvent(new CustomEvent('uclaw:stt-end'))
      return
    }
    if (modelStatus.kind !== 'ready') {
      onShowDownloadDialog?.()
      return
    }
    window.dispatchEvent(new CustomEvent('uclaw:stt-start'))
  }, [isBusyElsewhere, isOpenHere, modelStatus.kind, onShowDownloadDialog])

  // 全局快捷键 → 只有 chat-side 实例响应，避免两个 composer 都挂时双触发。
  useShortcut({
    id: 'toggle-stt-recording',
    handler: handleClick,
    disabled: composer !== 'chat',
  })

  const tooltipText =
    modelStatus.kind === 'ready'
      ? isOpenHere
        ? '结束语音输入'
        : '语音输入'
      : modelStatus.kind === 'downloading'
        ? `模型下载中… ${modelStatus.percent}%`
        : '语音输入（点击下载模型）'

  const Icon = modalState.kind === 'permission-denied' ? MicOff : Mic

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
            isOpenHere
              ? 'text-primary bg-primary/10 hover:bg-primary/20'
              : 'text-foreground/60 hover:text-foreground',
          )}
        >
          <Icon className="size-5" />
          {modelStatus.kind === 'not-downloaded' && !isOpenHere && (
            <span
              aria-hidden
              className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-primary"
            />
          )}
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

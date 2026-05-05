// [PLACEHOLDER] ai-elements/speech-button — 语音输入按钮
import * as React from 'react'
import { Mic } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

interface SpeechButtonProps {
  onTranscript: (text: string) => void
}

export function SpeechButton({ onTranscript }: SpeechButtonProps): React.ReactElement {
  // [PLACEHOLDER] 语音识别功能将在后续任务中实现
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-[30px] rounded-full text-foreground/60 hover:text-foreground"
          disabled
        >
          <Mic className="size-5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p>语音输入（即将推出）</p>
      </TooltipContent>
    </Tooltip>
  )
}

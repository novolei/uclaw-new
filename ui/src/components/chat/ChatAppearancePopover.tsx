/**
 * ChatAppearancePopover - 聊天外观调整弹窗
 *
 * 提供聊天内容的视觉调整：
 * - 字体大小（小 / 中 / 大）
 * - 衬线字体切换
 *
 * 设置实时应用到 <html> 的 data-chat-* 属性，由 globals.css 中
 * 对应的选择器响应。
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { ALargeSmall } from 'lucide-react'
import {
  chatFontSizeAtom,
  chatSerifAtom,
  applyChatAppearanceToDOM,
  updateChatFontSize,
  updateChatSerif,
  type ChatFontSize,
} from '@/atoms/chat-appearance'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

const FONT_SIZE_OPTIONS: Array<{ value: ChatFontSize; label: string; sample: string }> = [
  { value: 'sm', label: '小', sample: 'A' },
  { value: 'md', label: '中', sample: 'A' },
  { value: 'lg', label: '大', sample: 'A' },
]

const SAMPLE_PX: Record<ChatFontSize, number> = { sm: 12, md: 14, lg: 17 }

export function ChatAppearancePopover(): React.ReactElement {
  const [fontSize, setFontSize] = useAtom(chatFontSizeAtom)
  const [serif, setSerif] = useAtom(chatSerifAtom)

  // 同步到 DOM
  React.useEffect(() => {
    applyChatAppearanceToDOM(fontSize, serif)
  }, [fontSize, serif])

  const handleFontSize = (v: ChatFontSize): void => {
    setFontSize(v)
    updateChatFontSize(v)
  }

  const handleSerif = (v: boolean): void => {
    setSerif(v)
    updateChatSerif(v)
  }

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button type="button" variant="ghost" size="icon" className="h-7 w-7">
              <ALargeSmall className="size-4" />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="bottom"><p>聊天外观</p></TooltipContent>
      </Tooltip>
      <PopoverContent align="end" className="w-[260px] p-0">
        {/* 标题 */}
        <div className="px-3 py-2 border-b">
          <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            聊天外观
          </div>
        </div>

        {/* 字体大小 */}
        <div className="px-3 py-2.5 border-b">
          <div className="text-[12px] text-muted-foreground mb-1.5">字体大小</div>
          <div className="grid grid-cols-3 gap-1.5">
            {FONT_SIZE_OPTIONS.map((opt) => {
              const active = fontSize === opt.value
              return (
                <button
                  key={opt.value}
                  type="button"
                  onClick={() => handleFontSize(opt.value)}
                  className={cn(
                    'flex flex-col items-center justify-center gap-0.5 h-12 rounded-md border transition-all',
                    active
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border bg-muted/30 text-muted-foreground hover:bg-muted/60',
                  )}
                >
                  <span
                    className="font-semibold leading-none"
                    style={{ fontSize: SAMPLE_PX[opt.value] }}
                  >
                    {opt.sample}
                  </span>
                  <span className="text-[10px] leading-none">{opt.label}</span>
                </button>
              )
            })}
          </div>
        </div>

        {/* 衬线字体切换 */}
        <div className="px-3 py-2.5 flex items-center justify-between gap-3">
          <div>
            <div className="text-[13px] font-medium leading-tight">衬线字体</div>
            <div className="text-[11px] text-muted-foreground mt-0.5">
              使用 serif 字体，更适合长文阅读
            </div>
          </div>
          <Switch checked={serif} onCheckedChange={handleSerif} />
        </div>
      </PopoverContent>
    </Popover>
  )
}

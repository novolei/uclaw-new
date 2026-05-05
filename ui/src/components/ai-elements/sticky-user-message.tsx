/**
 * StickyUserMessage — 用户消息置顶条
 *
 * 当用户消息滚出可视区域时，在顶部显示一条精简预览。
 * 从 Proma 迁移。
 */

import * as React from 'react'
import { User, Paperclip } from 'lucide-react'
import { cn } from '@/lib/utils'

interface UserMessageData {
  id: string | null
  text: string
  attachments: Array<{ filename: string; isImage: boolean }>
}

interface StickyUserMessageProps {
  userMessages: UserMessageData[]
  className?: string
  onClick?: (id: string) => void
}

export function StickyUserMessage({
  userMessages,
  className,
  onClick,
}: StickyUserMessageProps): React.ReactElement | null {
  if (userMessages.length === 0) return null

  const latest = userMessages[userMessages.length - 1]!
  if (!latest.id) return null

  return (
    <div
      className={cn(
        'sticky top-0 z-20 px-4 py-1.5 bg-background/80 backdrop-blur-sm border-b border-border/40 cursor-pointer hover:bg-accent/30 transition-colors',
        className,
      )}
      onClick={() => latest.id && onClick?.(latest.id)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          latest.id && onClick?.(latest.id)
        }
      }}
    >
      <div className="flex items-center gap-2 max-w-full">
        <User className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="text-xs text-muted-foreground truncate flex-1">
          {latest.text.slice(0, 120)}
          {latest.text.length > 120 ? '...' : ''}
        </span>
        {latest.attachments.length > 0 && (
          <span className="flex items-center gap-0.5 text-[10px] text-muted-foreground/50 shrink-0">
            <Paperclip className="size-2.5" />
            {latest.attachments.length}
          </span>
        )}
      </div>
    </div>
  )
}

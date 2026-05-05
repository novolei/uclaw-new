// [PLACEHOLDER] ai-elements/message — 待后续任务迁移
import * as React from 'react'
import { cn } from '@/lib/utils'

// ===== Message 原语组件 =====

interface MessageProps {
  from: 'user' | 'assistant'
  children: React.ReactNode
}

export function Message({ from, children }: MessageProps): React.ReactElement {
  return (
    <div className={cn('px-4 py-3', from === 'user' ? 'bg-transparent' : '')}>
      {children}
    </div>
  )
}

export function MessageHeader({
  model,
  time,
  logo,
}: {
  model?: string
  time?: string
  logo?: React.ReactNode
}): React.ReactElement {
  return (
    <div className="flex items-start gap-2.5 mb-2.5">
      {logo}
      <div className="flex flex-col justify-between h-[35px]">
        <span className="text-sm font-semibold text-foreground/60 leading-none">{model || 'Assistant'}</span>
        {time && <span className="text-[10px] text-foreground/[0.38] leading-none">{time}</span>}
      </div>
    </div>
  )
}

export function MessageContent({ children, className }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <div className={cn('pl-[46px]', className)}>{children}</div>
}

export function MessageActions({
  children,
  className,
}: {
  children: React.ReactNode
  className?: string
}): React.ReactElement {
  return (
    <div className={cn('flex items-center gap-0.5 opacity-0 hover:opacity-100 transition-opacity', className)}>
      {children}
    </div>
  )
}

export function MessageAction({
  children,
  onClick,
  tooltip,
  disabled,
}: {
  children: React.ReactNode
  onClick?: () => void
  tooltip?: string
  disabled?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      className="p-1 rounded text-muted-foreground/60 hover:text-foreground transition-colors disabled:opacity-40"
      onClick={onClick}
      title={tooltip}
      disabled={disabled}
    >
      {children}
    </button>
  )
}

export function MessageResponse({
  children,
  basePath,
  basePaths,
}: {
  children: React.ReactNode
  basePath?: string
  basePaths?: string[]
}): React.ReactElement {
  return (
    <div className="prose prose-sm dark:prose-invert max-w-none prose-p:my-1 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 text-[15px] leading-relaxed">
      {typeof children === 'string' ? <p className="whitespace-pre-wrap">{children}</p> : children}
    </div>
  )
}

export function UserMessageContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <div className="text-[15px] leading-relaxed whitespace-pre-wrap break-words">
      {children}
    </div>
  )
}

export function BasePathsProvider({
  basePaths,
  children,
}: {
  basePaths?: string[]
  children: React.ReactNode
}): React.ReactElement {
  return <>{children}</>
}

// ===== 流式辅助组件 =====

export function MessageLoading({ startedAt }: { startedAt?: number }): React.ReactElement {
  return (
    <div className="flex items-center gap-1.5 py-1">
      <div className="flex gap-0.5">
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '150ms' }} />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '300ms' }} />
      </div>
    </div>
  )
}

export function StreamingIndicator(): React.ReactElement {
  return (
    <span className="inline-block ml-0.5 w-2 h-4 bg-foreground/40 animate-pulse rounded-sm" />
  )
}

export function MessageStopped(): React.ReactElement {
  return (
    <div className="text-sm text-foreground/40 italic">
      已中止生成
    </div>
  )
}

export function MessageAttachments({ attachments }: { attachments: Array<{ filename: string; mediaType: string; localPath: string }> }): React.ReactElement {
  return (
    <div className="flex flex-wrap gap-2 mt-2">
      {attachments.map((att, i) => (
        <div key={i} className="text-xs text-muted-foreground bg-muted/50 rounded px-2 py-1">
          {att.filename}
        </div>
      ))}
    </div>
  )
}

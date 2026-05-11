import * as React from 'react'
import { LoaderCircle, MoreHorizontal, FolderInput, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

interface SessionItemProps {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  isActive: boolean
  /** Whether the agent loop is currently running for this session. */
  running?: boolean
  onClick: () => void
  onDelete?: () => void
  onMove?: () => void
}

export function SessionItem({
  title,
  titleEmoji,
  titlePending,
  isActive,
  running,
  onClick,
  onDelete,
  onMove,
}: SessionItemProps): React.ReactElement {
  const hasMenu = Boolean(onDelete || onMove)
  return (
    <div
      onClick={onClick}
      className={cn(
        'group flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer',
        'text-[13px] transition-colors duration-100',
        isActive
          ? 'bg-sidebar-accent text-sidebar-primary font-medium'
          : 'text-muted-foreground hover:bg-muted hover:text-foreground'
      )}
    >
      <span className="shrink-0 inline-flex items-center justify-center text-primary" style={{ width: '18px' }}>
        {titlePending ? (
          <LoaderCircle size={14} strokeWidth={2} className="animate-spin" />
        ) : (
          <span className="text-[14px] leading-none" style={{ fontFamily: "'Noto Emoji', sans-serif" }}>
            {titleEmoji || '💬'}
          </span>
        )}
      </span>
      {titlePending ? (
        <span className="flex-1 h-3.5 rounded bg-muted-foreground/20 animate-pulse" />
      ) : (
        <span className="flex-1 truncate">{title || 'New session'}</span>
      )}
      {/* Always-visible running indicator — pulsing primary dot when this
          session has an active agent loop. Lets the user spot in-flight
          tasks across sessions without switching tabs. */}
      {running && !titlePending && (
        <span
          className="shrink-0 size-1.5 rounded-full bg-primary animate-pulse shadow-[0_0_6px_hsl(var(--primary))] group-hover:opacity-0 transition-opacity"
          title="任务执行中"
        />
      )}
      {hasMenu && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => e.stopPropagation()}
              className="shrink-0 opacity-60 hover:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded hover:bg-foreground/[0.08]"
              title="更多"
            >
              <MoreHorizontal className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" side="bottom" sideOffset={4} className="w-40 min-w-0 p-0.5 z-[100]">
            {onMove && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5"
                onSelect={(e) => {
                  console.debug('[session-menu] move clicked')
                  e.preventDefault()
                  onMove()
                }}
              >
                <FolderInput />
                移动到...
              </DropdownMenuItem>
            )}
            {onDelete && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
                onSelect={(e) => {
                  console.debug('[session-menu] delete clicked')
                  e.preventDefault()
                  onDelete()
                }}
              >
                <Trash2 />
                删除
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  )
}

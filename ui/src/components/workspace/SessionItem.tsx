import * as React from 'react'
import { LoaderCircle } from 'lucide-react'
import { cn } from '@/lib/utils'

interface SessionItemProps {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  isActive: boolean
  onClick: () => void
  onDelete?: () => void
}

export function SessionItem({
  title,
  titleEmoji,
  titlePending,
  isActive,
  onClick,
  onDelete,
}: SessionItemProps): React.ReactElement {
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
      {onDelete && (
        <button
          onClick={(e) => { e.stopPropagation(); onDelete(); }}
          className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive p-0.5 rounded"
        >
          ×
        </button>
      )}
    </div>
  )
}

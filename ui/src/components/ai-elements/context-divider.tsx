// [PLACEHOLDER] ai-elements/context-divider — 上下文分隔线
import * as React from 'react'
import { Scissors, X } from 'lucide-react'

interface ContextDividerProps {
  messageId: string
  onDelete?: (messageId: string) => void
  className?: string
}

export function ContextDivider({ messageId, onDelete, className }: ContextDividerProps): React.ReactElement {
  return (
    <div className={`flex items-center gap-2 my-2 px-4 ${className ?? ''}`}>
      <div className="flex-1 h-px bg-yellow-500/30" />
      <div className="flex items-center gap-1 text-xs text-yellow-600 dark:text-yellow-400">
        <Scissors className="size-3" />
        <span>上下文已清除</span>
        {onDelete && (
          <button
            type="button"
            className="p-0.5 rounded hover:bg-yellow-500/10"
            onClick={() => onDelete(messageId)}
          >
            <X className="size-3" />
          </button>
        )}
      </div>
      <div className="flex-1 h-px bg-yellow-500/30" />
    </div>
  )
}

import * as React from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
import type { PreviewTabItem as PreviewTabItemModel } from '@/atoms/preview-panel-atoms'
import { useWindowDragOnMove } from '@/hooks/useWindowDragOnMove'

interface Props {
  tab: PreviewTabItemModel
  isActive: boolean
  onActivate: () => void
  onClose: () => void
}

export function PreviewTabItem({
  tab,
  isActive,
  onActivate,
  onClose,
}: Props): React.ReactElement {
  const windowDrag = useWindowDragOnMove()

  return (
    <div
      role="tab"
      aria-selected={isActive}
      aria-label={tab.name}
      tabIndex={isActive ? 0 : -1}
      onClick={(e) => {
        if (windowDrag.consumeClickIfDragged(e)) return
        onActivate()
      }}
      onPointerDown={windowDrag.onPointerDown}
      onPointerMove={windowDrag.onPointerMove}
      onPointerUp={windowDrag.onPointerUp}
      onPointerCancel={windowDrag.onPointerCancel}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onActivate()
        }
      }}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault()
          onClose()
        }
      }}
      className={cn(
        'titlebar-no-drag window-drag-tab group flex items-center gap-1.5 px-3 py-1.5 text-xs select-none',
        'border-r border-border/40 min-w-[80px] max-w-[200px] shrink-0',
        windowDrag.isDragging && 'is-window-dragging',
        isActive
          ? 'bg-background text-foreground border-b-2 border-b-primary'
          : 'bg-card text-muted-foreground hover:bg-muted/40',
      )}
    >
      {tab.source === 'agent' && (
        <span
          aria-label="opened by agent"
          title="opened by agent"
          className="text-[10px] leading-none"
        >
          ✨
        </span>
      )}
      <FileTypeIcon name={tab.name} isDirectory={false} size={14} className="shrink-0" />
      <span className="truncate flex-1" title={tab.relPath}>
        {tab.name}
      </span>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          onClose()
        }}
        className={cn(
          'titlebar-no-drag size-4 flex items-center justify-center rounded shrink-0',
          'opacity-0 group-hover:opacity-100',
          isActive && 'opacity-100',
          'hover:bg-muted/60 transition-opacity',
        )}
        aria-label={`close ${tab.name}`}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}

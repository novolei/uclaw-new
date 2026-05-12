import * as React from 'react'
import { useSetAtom } from 'jotai'
import { X, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import { closePreviewAction, type PreviewFileTarget } from '@/atoms/preview-panel-atoms'

interface PreviewHeaderProps {
  target: PreviewFileTarget | null
  resolvedPath?: string
}

export function PreviewHeader({ target, resolvedPath }: PreviewHeaderProps): React.ReactElement {
  const closePreview = useSetAtom(closePreviewAction)
  const displayPath = resolvedPath ?? target?.relPath ?? ''

  return (
    <header className="flex items-center gap-2 h-[36px] flex-shrink-0 border-b border-border bg-popover px-3">
      <div className="flex-1 min-w-0 flex flex-col">
        <div className="text-[12px] font-medium text-foreground truncate">
          {target?.name ?? '未选中文件'}
        </div>
        {displayPath && (
          <div
            className="text-[10px] text-muted-foreground/70 truncate"
            dir="rtl"
            title={displayPath}
          >
            {displayPath}
          </div>
        )}
      </div>
      <button
        type="button"
        aria-label="弹出独立窗口 (W5 即将上线)"
        title="弹出独立窗口（W5 即将上线）"
        disabled
        className={cn(
          'size-7 inline-flex items-center justify-center rounded',
          'text-foreground/30 cursor-not-allowed',
        )}
      >
        <ExternalLink size={13} />
      </button>
      <button
        type="button"
        aria-label="关闭预览"
        onClick={() => closePreview()}
        className={cn(
          'size-7 inline-flex items-center justify-center rounded',
          'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.06]',
          'transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        )}
      >
        <X size={14} />
      </button>
    </header>
  )
}

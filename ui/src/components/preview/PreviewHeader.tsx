/**
 * PreviewHeader — Title row for the preview panel.
 *
 * Layout: [file-type icon] [name / path stack] [copy-path] [pop-out] [close].
 * File-type icon comes from the existing `<FileTypeIcon>` so it matches the
 * rail's iconography. Copy-path lifts the path to the clipboard with a brief
 * confirmation. Pop-out is a placeholder for W5's detached window.
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { X, ExternalLink, Copy, Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import { closePreviewAction, type PreviewFileTarget } from '@/atoms/preview-panel-atoms'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'

interface PreviewHeaderProps {
  target: PreviewFileTarget | null
  resolvedPath?: string
}

interface HeaderButtonProps {
  ariaLabel: string
  title: string
  onClick?: () => void
  disabled?: boolean
  children: React.ReactNode
}

function HeaderButton({
  ariaLabel,
  title,
  onClick,
  disabled = false,
  children,
}: HeaderButtonProps): React.ReactElement {
  return (
    <button
      type="button"
      aria-label={ariaLabel}
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'size-7 inline-flex items-center justify-center rounded-md',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        disabled
          ? 'text-foreground/25 cursor-not-allowed'
          : 'text-foreground/60 hover:text-foreground hover:bg-foreground/[0.06] active:bg-foreground/[0.10]',
      )}
    >
      {children}
    </button>
  )
}

export function PreviewHeader({ target, resolvedPath }: PreviewHeaderProps): React.ReactElement {
  const closePreview = useSetAtom(closePreviewAction)
  const displayPath = resolvedPath ?? target?.relPath ?? ''
  const [copied, setCopied] = React.useState(false)
  const copyResetTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  React.useEffect(() => {
    return () => {
      if (copyResetTimer.current) clearTimeout(copyResetTimer.current)
    }
  }, [])

  const handleCopy = React.useCallback(async () => {
    if (!displayPath) return
    try {
      await navigator.clipboard.writeText(displayPath)
      setCopied(true)
      if (copyResetTimer.current) clearTimeout(copyResetTimer.current)
      copyResetTimer.current = setTimeout(() => setCopied(false), 1400)
    } catch (err) {
      toast.error('复制失败', { description: String(err) })
    }
  }, [displayPath])

  return (
    <header
      className={cn(
        'flex items-center gap-2 flex-shrink-0',
        'h-[40px] px-3',
        'border-b border-border bg-popover',
      )}
    >
      <FileTypeIcon
        name={target?.name ?? 'unknown'}
        isDirectory={false}
        size={14}
        className="shrink-0"
      />
      <div className="flex-1 min-w-0 flex flex-col leading-tight">
        <div className="text-[12.5px] font-medium text-foreground truncate">
          {target?.name ?? '未选中文件'}
        </div>
        {displayPath && (
          <div
            className="text-[10.5px] text-muted-foreground/70 truncate font-mono tabular-nums"
            dir="rtl"
            title={displayPath}
          >
            {displayPath}
          </div>
        )}
      </div>
      {displayPath && (
        <HeaderButton
          ariaLabel={copied ? '路径已复制' : '复制完整路径'}
          title={copied ? '已复制' : '复制路径'}
          onClick={handleCopy}
        >
          {copied ? <Check size={13} className="text-emerald-500" /> : <Copy size={13} />}
        </HeaderButton>
      )}
      <HeaderButton
        ariaLabel="弹出独立窗口 (W5 即将上线)"
        title="弹出独立窗口（W5 即将上线）"
        disabled
      >
        <ExternalLink size={13} />
      </HeaderButton>
      <HeaderButton
        ariaLabel="关闭预览"
        title="关闭预览 (Esc)"
        onClick={() => closePreview()}
      >
        <X size={14} />
      </HeaderButton>
    </header>
  )
}

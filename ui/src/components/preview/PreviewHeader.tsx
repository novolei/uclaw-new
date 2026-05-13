/**
 * PreviewHeader — Title row for the preview panel.
 *
 * Layout (left → right): file-type icon · filename / parent-folder stack ·
 * action buttons (copy path, reveal in Finder, close). The parent-folder
 * subline replaces the old full-path display because for files at the
 * workspace root the path was identical to the filename and looked
 * redundant. Home directory is collapsed to `~/` for legibility.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { invoke } from '@tauri-apps/api/core'
import { X, FolderOpen, Copy, Check, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  closePreviewAction,
  pendingWriteToolsAtom,
  type PreviewFileTarget,
} from '@/atoms/preview-panel-atoms'
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'

interface PreviewHeaderProps {
  target: PreviewFileTarget | null
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
        'size-6 inline-flex items-center justify-center rounded-md shrink-0',
        'transition-colors motion-reduce:transition-none',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
        disabled
          ? 'text-foreground/25 cursor-not-allowed'
          : 'text-foreground/55 hover:text-foreground hover:bg-foreground/[0.06] active:bg-foreground/[0.10]',
      )}
    >
      {children}
    </button>
  )
}

/** Best-effort home-dir collapse — purely cosmetic for the path display. */
function prettifyPath(p: string): string {
  if (!p) return ''
  const unixHome = p.match(/^(\/Users\/[^/]+|\/home\/[^/]+)(\/.*)?$/)
  if (unixHome) return `~${unixHome[2] ?? ''}`
  const winHome = p.match(/^([A-Za-z]:\\Users\\[^\\]+)(\\.*)?$/)
  if (winHome) return `~${(winHome[2] ?? '').replace(/\\/g, '/')}`
  return p
}

function parentDir(p: string): string {
  if (!p) return ''
  const normalized = p.replace(/\\/g, '/')
  const idx = normalized.lastIndexOf('/')
  if (idx <= 0) return ''
  return normalized.slice(0, idx)
}

/**
 * Middle-truncate so both the leading marker (`~`, drive letter) and the
 * trailing parent folder stay visible. The previous `dir="rtl"` approach
 * fed the Unicode bidi algorithm an LTR string in an RTL paragraph and
 * mangled neutral characters — the leading `~/` rendered at the visual
 * end ("Documents/.../w4d-test/~"). Plain string truncation avoids bidi
 * entirely.
 */
function middleTruncate(s: string, max: number = 42): string {
  if (s.length <= max) return s
  const keepEnd = Math.floor((max - 1) * 0.7)
  const keepStart = max - 1 - keepEnd
  return `${s.slice(0, keepStart)}…${s.slice(s.length - keepEnd)}`
}

export function PreviewHeader({ target }: PreviewHeaderProps): React.ReactElement {
  const closePreview = useSetAtom(closePreviewAction)
  const absolutePath = target?.absolutePath ?? ''
  const parent = parentDir(absolutePath)
  const prettyParent = middleTruncate(prettifyPath(parent), 48)
  const [copied, setCopied] = React.useState(false)
  const copyResetTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // Show a spinner when there is an in-flight write tool targeting the file
  // currently being previewed. The pendingWriteToolsAtom map is populated by
  // the auto-preview listener on tool_start and cleared on tool_result.
  const pendingWrites = useAtomValue(pendingWriteToolsAtom)
  const sid = target?.sessionId ?? null
  const writeInFlight = React.useMemo(() => {
    if (!sid || !absolutePath) return false
    const inner = pendingWrites.get(sid)
    if (!inner) return false
    for (const path of inner.values()) {
      if (path === absolutePath) return true
    }
    return false
  }, [pendingWrites, sid, absolutePath])

  React.useEffect(() => {
    return () => {
      if (copyResetTimer.current) clearTimeout(copyResetTimer.current)
    }
  }, [])

  const handleCopy = React.useCallback(async () => {
    if (!absolutePath) return
    try {
      await navigator.clipboard.writeText(absolutePath)
      setCopied(true)
      if (copyResetTimer.current) clearTimeout(copyResetTimer.current)
      copyResetTimer.current = setTimeout(() => setCopied(false), 1400)
    } catch (err) {
      toast.error('复制失败', { description: String(err) })
    }
  }, [absolutePath])

  const handleReveal = React.useCallback(async () => {
    if (!absolutePath) return
    try {
      await invoke('reveal_path_in_file_manager', { path: absolutePath })
    } catch (err) {
      toast.error('无法在文件管理器中显示', {
        description: err instanceof Error ? err.message : String(err),
      })
    }
  }, [absolutePath])

  const canReveal = absolutePath.length > 0

  return (
    <header
      className={cn(
        'flex items-center gap-2 flex-shrink-0',
        'h-[34px] px-3',
        'border-b border-border tabbar-bg',
      )}
    >
      <FileTypeIcon
        name={target?.name ?? 'unknown'}
        isDirectory={false}
        size={14}
        className="shrink-0"
      />
      <div className="flex-1 min-w-0 flex items-baseline gap-2">
        <span
          className="text-[12.5px] font-medium text-foreground truncate"
          title={absolutePath || (target?.name ?? '')}
        >
          {target?.name ?? '未选中文件'}
        </span>
        {writeInFlight && (
          <span
            className="inline-flex items-center gap-1 text-[10.5px] text-muted-foreground/80 shrink-0"
            title="Agent 正在写入此文件"
            aria-live="polite"
          >
            <Loader2 size={10} className="animate-spin" />
            <span>写入中</span>
          </span>
        )}
        {prettyParent && (
          <span
            className="hidden md:inline text-[10.5px] text-muted-foreground/70 truncate font-mono tabular-nums"
            title={absolutePath}
          >
            {prettyParent}
          </span>
        )}
      </div>
      {absolutePath && (
        <HeaderButton
          ariaLabel={copied ? '路径已复制' : '复制完整路径'}
          title={copied ? '已复制' : '复制完整路径'}
          onClick={handleCopy}
        >
          {copied ? <Check size={13} className="text-emerald-500" /> : <Copy size={13} />}
        </HeaderButton>
      )}
      <HeaderButton
        ariaLabel="在文件管理器中显示"
        title="在文件管理器中显示"
        onClick={handleReveal}
        disabled={!canReveal}
      >
        <FolderOpen size={14} />
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

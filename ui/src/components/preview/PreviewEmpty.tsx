/**
 * PreviewEmpty — Empty / loading / error states for the preview panel.
 *
 * Visual rhythm: badge-style icon container + 12.5px primary text + 11px
 * subtitle. The idle state uses a rounded muted badge for a more polished
 * "nothing selected" feel.
 */

import * as React from 'react'
import { FileText, AlertTriangle, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'

export interface PreviewEmptyProps {
  status: 'idle' | 'loading' | 'error'
  message?: string
}

interface StateShellProps {
  iconSlot: React.ReactNode
  title: string
  subtitle?: string | null
  tone?: 'muted' | 'danger'
}

function StateShell({
  iconSlot,
  title,
  subtitle,
  tone = 'muted',
}: StateShellProps): React.ReactElement {
  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center h-full',
        'p-8 text-center select-none bg-popover',
      )}
    >
      <div className="mb-3">{iconSlot}</div>
      <div
        className={cn(
          'text-[12.5px] font-medium',
          tone === 'danger' ? 'text-destructive' : 'text-foreground/75',
        )}
      >
        {title}
      </div>
      {subtitle && (
        <div
          className={cn(
            'mt-1.5 text-[11px] max-w-[280px] leading-relaxed break-words',
            tone === 'danger' ? 'text-muted-foreground' : 'text-muted-foreground/65',
          )}
        >
          {subtitle}
        </div>
      )}
    </div>
  )
}

export function PreviewEmpty({ status, message }: PreviewEmptyProps): React.ReactElement {
  if (status === 'loading') {
    return (
      <StateShell
        iconSlot={
          <Loader2
            className="size-7 text-foreground/40 animate-spin motion-reduce:animate-none"
            aria-hidden
          />
        }
        title="正在读取文件…"
      />
    )
  }
  if (status === 'error') {
    return (
      <StateShell
        tone="danger"
        iconSlot={<AlertTriangle className="size-7 text-destructive" aria-hidden />}
        title="读取失败"
        subtitle={message ?? '未知错误'}
      />
    )
  }
  return (
    <StateShell
      iconSlot={
        <div className="size-12 rounded-full bg-muted/50 flex items-center justify-center">
          <FileText className="size-6 text-muted-foreground/55" aria-hidden />
        </div>
      }
      title="还没选中文件"
      subtitle="在右侧文件树点击任意文件开始预览"
    />
  )
}

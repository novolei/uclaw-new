import * as React from 'react'
import { Plus, Minus, Pencil, ArrowRight } from 'lucide-react'
import { cn } from '@/lib/utils'

export type FileChangeBadge = 'created' | 'modified' | 'removed' | 'renamed'

interface ChangeRowProps {
  badge: FileChangeBadge
  path: string
  newPath?: string
  onClick?: () => void
}

const BADGE_META: Record<FileChangeBadge, { Icon: typeof Plus; label: string; cls: string }> = {
  created: { Icon: Plus, label: '新增', cls: 'text-[hsl(var(--success))]' },
  removed: { Icon: Minus, label: '删除', cls: 'text-destructive' },
  modified: { Icon: Pencil, label: '修改', cls: 'text-foreground/70' },
  renamed: { Icon: ArrowRight, label: '重命名', cls: 'text-foreground/70' },
}

export function ChangeRow({ badge, path, newPath, onClick }: ChangeRowProps): React.ReactElement {
  const meta = BADGE_META[badge]
  const Icon = meta.Icon
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center w-full h-[26px] px-3 gap-2 text-[12px] text-left',
        'text-foreground/85 hover:bg-foreground/[0.04] transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
      )}
      title={newPath ? `${path} → ${newPath}` : path}
    >
      <span className={cn('shrink-0', meta.cls)} aria-label={meta.label}>
        <Icon size={12} />
      </span>
      <span className="truncate font-mono tabular-nums" dir="rtl">
        {newPath ? `${path} → ${newPath}` : path}
      </span>
    </button>
  )
}

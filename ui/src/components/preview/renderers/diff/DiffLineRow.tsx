import * as React from 'react'
import { cn } from '@/lib/utils'
import type { DiffLine } from './useDiffHunks'

interface Props {
  line: DiffLine
  /** Which column (left = old, right = new). For ctx, render in both. */
  column: 'left' | 'right'
}

export function DiffLineRow({ line, column }: Props): React.ReactElement {
  // For side-by-side: ctx + del go in left, ctx + add go in right.
  const visible =
    line.kind === 'ctx' ||
    (column === 'left' && line.kind === 'del') ||
    (column === 'right' && line.kind === 'add')

  if (!visible) {
    return <div className="h-[18px] bg-muted/30" aria-hidden />
  }

  const tint =
    line.kind === 'add'
      ? 'bg-emerald-100/60 dark:bg-emerald-900/30'
      : line.kind === 'del'
        ? 'bg-rose-100/60 dark:bg-rose-900/30'
        : ''

  const no = column === 'left' ? line.oldNo : line.newNo

  return (
    <div className={cn('flex items-start h-[18px] font-mono text-[11.5px] leading-[18px]', tint)}>
      <span className="w-10 shrink-0 select-none text-right pr-2 text-muted-foreground/70 tabular-nums">
        {no ?? ''}
      </span>
      <span className="w-3 shrink-0 select-none text-muted-foreground">
        {line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}
      </span>
      <span className="flex-1 whitespace-pre overflow-x-auto">{line.text}</span>
    </div>
  )
}

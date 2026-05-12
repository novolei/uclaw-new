import * as React from 'react'
import { cn } from '@/lib/utils'

interface Props {
  totals: { add: number; del: number }
  cellCount?: number
}

export function DiffDensityCells({ totals, cellCount = 12 }: Props): React.ReactElement {
  const total = totals.add + totals.del
  const cells = React.useMemo(() => {
    if (total === 0) return Array(cellCount).fill('none' as const)
    const addRatio = totals.add / total
    return Array.from({ length: cellCount }, (_, i) => {
      if (totals.add > 0 && totals.del > 0) {
        return i / cellCount < addRatio ? ('add' as const) : ('del' as const)
      }
      return totals.add > 0 ? ('add' as const) : ('del' as const)
    })
  }, [totals.add, totals.del, total, cellCount])

  return (
    <div className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] text-muted-foreground">
      <div className="flex h-2.5 gap-[1px]">
        {cells.map((c, i) => (
          <span
            key={i}
            className={cn(
              'w-2',
              c === 'add' && 'bg-emerald-500/70',
              c === 'del' && 'bg-rose-500/70',
              c === 'none' && 'bg-foreground/10',
            )}
          />
        ))}
      </div>
      <span>+{totals.add} -{totals.del}</span>
    </div>
  )
}

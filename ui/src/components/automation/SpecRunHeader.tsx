import { Play, Square } from 'lucide-react'

interface Props {
  specName: string
  onRun: () => void
  onStop?: () => void
  isRunning: boolean
  hasActiveRun?: boolean
  isStopping?: boolean
}

export function SpecRunHeader({
  specName,
  onRun,
  onStop,
  isRunning,
  hasActiveRun = false,
  isStopping = false,
}: Props) {
  return (
    <div className="titlebar-drag-region flex items-center justify-between px-3 py-2 border-b border-border/50 shrink-0">
      <span className="font-semibold text-sm truncate">{specName}</span>
      <div className="titlebar-no-drag flex items-center gap-2">
        {hasActiveRun && onStop && (
          <button
            onClick={onStop}
            disabled={isStopping}
            className="flex items-center gap-1 px-3 py-1 rounded-md border border-destructive/40 text-destructive text-xs hover:bg-destructive/10 disabled:opacity-60"
          >
            <Square size={12} />
            {isStopping ? '停止中…' : '停止'}
          </button>
        )}
        <button
          onClick={onRun}
          disabled={isRunning || hasActiveRun}
          className="flex items-center gap-1 px-3 py-1 rounded-md bg-primary text-primary-foreground text-xs disabled:opacity-60"
        >
          <Play size={12} />
          {isRunning ? '运行中…' : '运行'}
        </button>
      </div>
    </div>
  )
}

interface Props {
  specName: string
  onRun: () => void
  isRunning: boolean
}

export function SpecRunHeader({ specName, onRun, isRunning }: Props) {
  return (
    <div className="titlebar-drag-region flex items-center justify-between px-3 py-2 border-b border-border/50 shrink-0">
      <span className="font-semibold text-sm truncate">{specName}</span>
      <button
        onClick={onRun}
        disabled={isRunning}
        className="titlebar-no-drag flex items-center gap-1 px-3 py-1 rounded-md bg-primary text-primary-foreground text-xs disabled:opacity-60"
      >
        {isRunning ? '运行中…' : '▶ 运行'}
      </button>
    </div>
  )
}

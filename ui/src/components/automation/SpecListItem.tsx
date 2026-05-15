import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  spec: HumaneSpecRow
  isSelected: boolean
  onSelect: () => void
  onRun: () => void
}

const STATUS_DOT: Record<string, string> = {
  active: 'bg-green-500',
  paused: 'bg-yellow-500',
  error: 'bg-red-500',
}

export function SpecListItem({ spec, isSelected, onSelect, onRun }: Props) {
  return (
    <button
      onClick={onSelect}
      className={[
        'group w-full text-left px-3 py-2 rounded-lg border transition-colors',
        'hover:bg-accent/50',
        isSelected
          ? 'border-primary bg-primary/5'
          : 'border-transparent',
      ].join(' ')}
    >
      <div className="flex items-center gap-2">
        <span
          className={[
            'h-2 w-2 rounded-full shrink-0',
            STATUS_DOT[spec.status] ?? 'bg-muted-foreground',
          ].join(' ')}
        />
        <span className="flex-1 truncate text-sm font-medium">{spec.name}</span>
        <div
          onClick={(e) => { e.stopPropagation(); onRun() }}
          className="titlebar-no-drag hidden group-hover:flex items-center gap-1 px-2 py-0.5 rounded text-xs bg-primary text-primary-foreground cursor-pointer"
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.stopPropagation()
              onRun()
            }
          }}
        >
          ▶
        </div>
      </div>
    </button>
  )
}

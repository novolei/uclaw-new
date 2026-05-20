import type { HumaneSpecRow } from '@/lib/tauri-bridge'

interface Props {
  spec: HumaneSpecRow
  isSelected: boolean
  onSelect: () => void
  onRun: () => void
}

const STATUS_DOT: Record<string, string> = {
  active: 'bg-success',
  paused: 'bg-warning',
  error: 'bg-danger',
}

function liveSpecLabel(spec: HumaneSpecRow): string | null {
  try {
    const raw = JSON.parse(spec.specJson)
    if (raw?.x_uclaw_runtime?.kind !== 'live_room_moderator') return null
    const config = raw.config ?? {}
    const platform = config.platform ?? 'douyin'
    const roomId = config.room_id ?? config.roomId
    return roomId ? `${platform} · ${roomId}` : `${platform} · 未设置房间`
  } catch {
    return null
  }
}

export function SpecListItem({ spec, isSelected, onSelect, onRun }: Props) {
  const liveLabel = liveSpecLabel(spec)
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
            STATUS_DOT[spec.status] ?? 'bg-muted',
          ].join(' ')}
        />
        <span className="flex-1 min-w-0">
          <span className="block truncate text-sm font-medium">{spec.name}</span>
          {liveLabel && <span className="block truncate text-[10px] text-muted-foreground">{liveLabel}</span>}
        </span>
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

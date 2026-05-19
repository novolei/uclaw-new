import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  internetOnlineAtom,
  backendOnlineAtom,
  memuOnlineAtom,
} from '@/atoms/dock-atoms'

type BarState = 'online' | 'warning' | 'offline'

interface BarConfig {
  channel: 'internet' | 'backend' | 'memu'
  label: string
  state: BarState
  height: number
  statusText: string
}

function SignalBarVisual({ channel, state, height }: { channel: string; state: BarState; height: number }) {
  return (
    <span
      data-conn-bar={channel}
      data-state={state}
      style={{ height }}
      className={cn(
        'block w-[3px] rounded-[1.5px] transition-colors duration-200',
        state === 'online' && 'bg-sage-500 shadow-[0_0_3px_-1px_theme(colors.sage.500)]',
        state === 'warning' && 'bg-amber-500',
        state === 'offline' && 'bg-coral-500',
      )}
    />
  )
}

const STATE_DOT_CLASS: Record<BarState, string> = {
  online: 'bg-sage-500',
  warning: 'bg-amber-500',
  offline: 'bg-coral-500',
}

export function ConnectionIndicator() {
  const internet = useAtomValue(internetOnlineAtom)
  const backend = useAtomValue(backendOnlineAtom)
  const memu = useAtomValue(memuOnlineAtom)

  const netState: BarState = internet ? 'online' : 'offline'
  const backendState: BarState = !internet
    ? 'offline'
    : backend
      ? 'online'
      : 'offline'
  const memuState: BarState = !internet
    ? 'offline'
    : memu === null
      ? 'warning'
      : memu
        ? 'online'
        : 'offline'

  const bars: BarConfig[] = [
    {
      channel: 'internet',
      label: '网络',
      state: netState,
      height: 6,
      statusText: internet ? '在线' : '离线',
    },
    {
      channel: 'backend',
      label: '后端',
      state: backendState,
      height: 10,
      statusText: !internet ? '离线' : backend ? '在线' : '离线',
    },
    {
      channel: 'memu',
      label: 'memU',
      state: memuState,
      height: 14,
      statusText: !internet
        ? '离线'
        : memu === null
          ? '初始化中'
          : memu
            ? '在线'
            : '离线',
    },
  ]

  return (
    <TooltipProvider delayDuration={220}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            className="flex items-end gap-[2px] h-[18px] cursor-default"
            aria-label="连接状态"
            role="group"
          >
            {bars.map((b) => (
              <SignalBarVisual
                key={b.channel}
                channel={b.channel}
                state={b.state}
                height={b.height}
              />
            ))}
          </div>
        </TooltipTrigger>
        <TooltipContent
          side="top"
          sideOffset={8}
          className="text-[11px] px-2.5 py-2 rounded-md bg-popover/95 text-popover-foreground border border-border/60 shadow-md min-w-[140px]"
        >
          <div className="font-medium mb-1.5 opacity-80">连接状态</div>
          <ul className="space-y-1">
            {bars.map((b) => (
              <li key={b.channel} className="flex items-center justify-between gap-3">
                <span className="flex items-center gap-2">
                  <span className={cn('block w-1.5 h-1.5 rounded-full', STATE_DOT_CLASS[b.state])} />
                  <span>{b.label}</span>
                </span>
                <span className="opacity-70">{b.statusText}</span>
              </li>
            ))}
          </ul>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

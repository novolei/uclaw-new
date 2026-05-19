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

interface SignalBarProps {
  channel: 'internet' | 'backend' | 'memu'
  state: BarState
  /** Bar height in px — 6, 10, or 14 (creates the rising signal-strength shape) */
  height: number
  tooltipText: string
}

function SignalBar({ channel, state, height, tooltipText }: SignalBarProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          data-conn-bar={channel}
          data-state={state}
          role="img"
          aria-label={tooltipText}
          style={{ height }}
          className={cn(
            'block w-[3px] rounded-[1.5px] transition-colors duration-200',
            state === 'online' && 'bg-sage-500 shadow-[0_0_3px_-1px_theme(colors.sage.500)]',
            state === 'warning' && 'bg-amber-500',
            state === 'offline' && 'bg-coral-500',
          )}
        />
      </TooltipTrigger>
      <TooltipContent
        side="top"
        sideOffset={6}
        className="text-[11px] px-2 py-1 rounded-md bg-popover/95 text-popover-foreground border border-border/60 shadow-md"
      >
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  )
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

  return (
    <TooltipProvider delayDuration={220}>
      <div
        className="flex items-end gap-[2px] h-[18px]"
        aria-label="连接状态"
        role="group"
      >
        <SignalBar
          channel="internet"
          state={netState}
          height={6}
          tooltipText={`网络：${internet ? '在线' : '离线'}`}
        />
        <SignalBar
          channel="backend"
          state={backendState}
          height={10}
          tooltipText={`后端：${!internet ? '离线' : backend ? '在线' : '离线'}`}
        />
        <SignalBar
          channel="memu"
          state={memuState}
          height={14}
          tooltipText={`memU：${!internet ? '离线' : memu === null ? '初始化中' : memu ? '在线' : '离线'}`}
        />
      </div>
    </TooltipProvider>
  )
}

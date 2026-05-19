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

type DotState = 'online' | 'warning' | 'offline'

function StatusDot({
  tooltipText,
  state,
}: {
  tooltipText: string
  state: DotState
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn(
            'block w-[7px] h-[7px] rounded-full cursor-default ring-1 ring-inset',
            state === 'online' &&
              'bg-sage-500 ring-sage-500/40 shadow-[0_0_5px_-1px_theme(colors.sage.500)]',
            state === 'warning' &&
              'bg-amber-500 ring-amber-500/40',
            state === 'offline' &&
              'bg-coral-500 ring-coral-500/40',
          )}
          aria-label={tooltipText}
          role="status"
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

  const netState: DotState = internet ? 'online' : 'offline'
  const backendState: DotState = !internet
    ? 'offline'
    : backend
      ? 'online'
      : 'offline'
  const memuState: DotState = !internet
    ? 'offline'
    : memu === null
      ? 'warning'
      : memu
        ? 'online'
        : 'offline'

  return (
    <TooltipProvider delayDuration={220}>
      <div
        className="flex items-center gap-[5px]"
        aria-label="连接状态"
        role="group"
      >
        <StatusDot
          state={netState}
          tooltipText={`网络：${internet ? '在线' : '离线'}`}
        />
        <StatusDot
          state={backendState}
          tooltipText={`后端：${!internet ? '离线' : backend ? '在线' : '离线'}`}
        />
        <StatusDot
          state={memuState}
          tooltipText={`memU：${!internet ? '离线' : memu === null ? '初始化中' : memu ? '在线' : '离线'}`}
        />
      </div>
    </TooltipProvider>
  )
}

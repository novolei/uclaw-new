import { useAtomValue } from 'jotai'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { internetOnlineAtom, backendOnlineAtom, memuOnlineAtom } from '@/atoms/dock-atoms'

type DotState = 'online' | 'warning' | 'offline'

function StatusDot({ tooltipText, state }: { tooltipText: string; state: DotState }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn(
            'block w-1.5 h-1.5 rounded-full cursor-default',
            state === 'online' && 'bg-sage-500 shadow-[0_0_4px_theme(colors.sage.500)]',
            state === 'warning' && 'bg-amber-500',
            state === 'offline' && 'bg-coral-500',
          )}
        />
      </TooltipTrigger>
      <TooltipContent side="top">
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
  const backendState: DotState = !internet ? 'offline' : backend ? 'online' : 'offline'
  const memuState: DotState = !internet ? 'offline' : memu === null ? 'warning' : memu ? 'online' : 'offline'

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex items-center gap-1.5" aria-label="连接状态">
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

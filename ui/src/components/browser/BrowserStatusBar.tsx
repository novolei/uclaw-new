import * as React from 'react'
import { Layers, Eye, EyeOff } from 'lucide-react'
import { useAtom, useAtomValue } from 'jotai'
import { browserDOMOverlayVisibleAtom, browserDOMStateAtom } from '@/atoms/browser-atoms'
import type { StartupRuntimePackStatusReport } from '@/lib/startup/startup-doctor'
import { cn } from '@/lib/utils'

interface BrowserStatusBarProps {
  sessionId: string
  isLoading?: boolean
  runtimeStatus?: StartupRuntimePackStatusReport
  runtimeStatusLoading?: boolean
  runtimeStatusError?: string | null
}

export function BrowserStatusBar({
  sessionId,
  isLoading,
  runtimeStatus,
  runtimeStatusLoading,
  runtimeStatusError,
}: BrowserStatusBarProps): React.ReactElement {
  const [overlayVisible, setOverlayVisible] = useAtom(browserDOMOverlayVisibleAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const domEntry = domMap.get(sessionId)
  const elementCount = domEntry?.elements.length ?? 0
  const runtime = browserRuntimeStatusBarView(runtimeStatus, runtimeStatusLoading, runtimeStatusError, isLoading)

  return (
    <div className="flex items-center gap-2 px-3 py-1 border-t border-border/40 bg-muted/20 text-[11px] text-muted-foreground">
      <span className="flex items-center gap-1" title={runtime.title}>
        <span className={cn('inline-block w-1.5 h-1.5 rounded-full', runtime.dotClass)} />
        {runtime.label}
      </span>
      <span className="flex-1" />
      {elementCount > 0 && (
        <span className="flex items-center gap-1 opacity-70">
          <Layers size={10} />
          {elementCount} 个元素
        </span>
      )}
      <button
        onClick={() => setOverlayVisible((v) => !v)}
        className={cn(
          'flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors',
          overlayVisible ? 'bg-blue-500/20 text-blue-400' : 'hover:bg-accent text-muted-foreground',
        )}
        title={overlayVisible ? '隐藏元素标注' : '显示元素标注'}
      >
        {overlayVisible ? <Eye size={10} /> : <EyeOff size={10} />}
        标注
      </button>
    </div>
  )
}

function browserRuntimeStatusBarView(
  report: StartupRuntimePackStatusReport | undefined,
  runtimeStatusLoading: boolean | undefined,
  runtimeStatusError: string | null | undefined,
  pageLoading: boolean | undefined,
): { label: string; title: string; dotClass: string } {
  if (runtimeStatusError) {
    return {
      label: 'Runtime unavailable',
      title: runtimeStatusError,
      dotClass: 'bg-red-500',
    }
  }

  if (runtimeStatusLoading) {
    return {
      label: 'Runtime checking',
      title: 'Browser Runtime Supervisor status is loading',
      dotClass: 'bg-amber-500 animate-pulse',
    }
  }

  if (report?.supervisor) {
    const state = report.supervisor.runtimeState
    const active = report.supervisor.activeContextCount
    return {
      label: `Runtime ${runtimeStateLabel(state)}`,
      title: [
        `provider=${report.supervisor.providerId}`,
        `doctor=${report.supervisor.doctorStatus}`,
        `activeContexts=${active}`,
        report.supervisor.detail,
      ].filter(Boolean).join(' | '),
      dotClass: runtimeStateDotClass(state),
    }
  }

  if (pageLoading) {
    return {
      label: '加载中...',
      title: 'Browser page is loading',
      dotClass: 'bg-amber-500 animate-pulse',
    }
  }

  return {
    label: '就绪',
    title: 'Browser panel is ready',
    dotClass: 'bg-green-500',
  }
}

function runtimeStateLabel(state: string): string {
  switch (state) {
    case 'ready':
    case 'idle':
      return 'ready'
    case 'acting':
      return 'acting'
    case 'recovering':
      return 'recovering'
    case 'degraded':
      return 'degraded'
    case 'starting':
      return 'starting'
    case 'stopped':
      return 'stopped'
    default:
      return state
  }
}

function runtimeStateDotClass(state: string): string {
  switch (state) {
    case 'ready':
    case 'idle':
      return 'bg-green-500'
    case 'acting':
    case 'starting':
    case 'recovering':
      return 'bg-amber-500 animate-pulse'
    case 'degraded':
    case 'stopped':
      return 'bg-red-500'
    default:
      return 'bg-muted-foreground'
  }
}

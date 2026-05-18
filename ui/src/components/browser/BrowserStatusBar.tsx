import * as React from 'react'
import { Layers, Eye, EyeOff } from 'lucide-react'
import { useAtom, useAtomValue } from 'jotai'
import { browserDOMOverlayVisibleAtom, browserDOMStateAtom } from '@/atoms/browser-atoms'
import { cn } from '@/lib/utils'

interface BrowserStatusBarProps {
  sessionId: string
  isLoading?: boolean
}

export function BrowserStatusBar({ sessionId, isLoading }: BrowserStatusBarProps): React.ReactElement {
  const [overlayVisible, setOverlayVisible] = useAtom(browserDOMOverlayVisibleAtom)
  const domMap = useAtomValue(browserDOMStateAtom)
  const domEntry = domMap.get(sessionId)
  const elementCount = domEntry?.elements.length ?? 0

  return (
    <div className="flex items-center gap-2 px-3 py-1 border-t border-border/40 bg-muted/20 text-[11px] text-muted-foreground">
      {isLoading ? (
        <span className="flex items-center gap-1">
          <span className="inline-block w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse" />
          加载中...
        </span>
      ) : (
        <span className="flex items-center gap-1">
          <span className="inline-block w-1.5 h-1.5 rounded-full bg-green-500" />
          就绪
        </span>
      )}
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

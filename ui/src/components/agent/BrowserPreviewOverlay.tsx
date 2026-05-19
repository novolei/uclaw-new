/**
 * BrowserPreviewOverlay — floating live preview of the agent's browser.
 *
 * Renders in the top-right corner of the AgentView. Shows a ~15 FPS JPEG
 * stream via CDP Screencast (browser:screencast-frame events). Collapses to
 * compact mode (URL bar only, no image) when a BrowserPanel tab is already
 * open for the same session — the full view is redundant at that point.
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { Globe, Minimize2, Maximize2, X, ExternalLink, MonitorPlay } from 'lucide-react'
import { cn } from '@/lib/utils'
import { sessionBrowserPreviewMapAtom } from '@/atoms/agent-atoms'
import { browserScreencastFrameAtom } from '@/atoms/browser-atoms'
import { activePreviewTabKeyAtom, previewTabKey } from '@/atoms/preview-panel-atoms'
import { useBrowserScreencast } from '@/hooks/useBrowserScreencast'

interface BrowserPreviewOverlayProps {
  sessionId: string
}

export function BrowserPreviewOverlay({ sessionId }: BrowserPreviewOverlayProps): React.ReactElement | null {
  const [previewMap, setPreviewMap] = useAtom(sessionBrowserPreviewMapAtom)
  const frameMap = useAtomValue(browserScreencastFrameAtom)
  const activeKey = useAtomValue(activePreviewTabKeyAtom)
  const preview = previewMap.get(sessionId)
  const tabId = preview?.visible ? (preview.tabId ?? null) : null
  useBrowserScreencast(sessionId, tabId)

  if (!preview?.visible) return null

  const { url, minimized } = preview

  // The full Browser panel may be open too, but this overlay remains a live
  // visual confirmation instead of collapsing into a status-only chip.
  const panelKey = previewTabKey({ mountId: 'browser', relPath: sessionId })
  const panelActive = activeKey === panelKey

  // Latest screencast frame for this session.
  const frame = frameMap.get(sessionId)

  const canvasRef = React.useRef<HTMLCanvasElement>(null)
  const lastDimsRef = React.useRef({ w: 0, h: 0 })

  React.useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !frame) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const binary = atob(frame.dataB64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
    const blob = new Blob([bytes], { type: frame.mimeType ?? 'image/jpeg' })
    let cancelled = false
    createImageBitmap(blob).then((bitmap) => {
      if (cancelled) { bitmap.close(); return }
      if (lastDimsRef.current.w !== bitmap.width || lastDimsRef.current.h !== bitmap.height) {
        canvas.width = bitmap.width
        canvas.height = bitmap.height
        lastDimsRef.current = { w: bitmap.width, h: bitmap.height }
      }
      ctx.drawImage(bitmap, 0, 0)
      bitmap.close()
    }).catch(() => {})
    return () => { cancelled = true }
  }, [frame])

  const update = (patch: Partial<typeof preview>) => {
    setPreviewMap((prev) => {
      const map = new Map(prev)
      map.set(sessionId, { ...preview, ...patch })
      return map
    })
  }

  const hostname = (() => {
    if (!url) return null
    try { return new URL(url).hostname } catch { return url }
  })()

  const isCollapsed = minimized

  return (
    <div
      className={cn(
        'absolute top-3 right-14 z-20',
        'flex flex-col rounded-xl overflow-hidden',
        'shadow-[0_8px_32px_rgba(0,0,0,0.18)] border border-border/60',
        'bg-popover backdrop-blur-sm',
        'transition-all duration-200 ease-out',
        isCollapsed ? 'w-[220px]' : 'w-[280px]',
      )}
    >
      {/* URL bar */}
      <div className="flex items-center gap-1.5 px-2.5 py-2 border-b border-border/40 bg-muted/40">
        <span className="relative flex h-2 w-2 shrink-0">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-60" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
        </span>
        <Globe size={11} className="shrink-0 text-muted-foreground" />
        <span className="flex-1 truncate text-[11px] text-muted-foreground leading-none select-none">
          {hostname ?? '浏览器运行中'}
        </span>
        {panelActive && (
          <span className="text-[10px] text-muted-foreground opacity-60 select-none shrink-0">
            面板已打开
          </span>
        )}
        {url && !panelActive && (
          <button
            onClick={() => window.open(url, '_blank')}
            className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title="在系统浏览器中打开"
          >
            <ExternalLink size={11} />
          </button>
        )}
        {!panelActive && (
          <button
            onClick={() => update({ minimized: !minimized })}
            className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title={minimized ? '展开' : '最小化'}
          >
            {minimized ? <Maximize2 size={11} /> : <Minimize2 size={11} />}
          </button>
        )}
        <button
          onClick={() => update({ visible: false })}
          className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
          title="关闭预览"
        >
          <X size={11} />
        </button>
      </div>

      {/* Screencast canvas — hidden when collapsed */}
      {!isCollapsed && (
        <div className="relative bg-muted/20" style={{ aspectRatio: '16/10' }}>
          {frame ? (
            <canvas
              ref={canvasRef}
              className="w-full h-full object-cover object-top"
              style={{ display: 'block' }}
            />
          ) : (
            <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
              <MonitorPlay size={22} className="opacity-30" />
              <span className="text-[11px] opacity-50">等待画面...</span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

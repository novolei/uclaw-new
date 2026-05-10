/**
 * BrowserPreviewOverlay — 浮动在聊天窗口右上角的 AI 浏览器预览面板
 *
 * 当 Agent 使用 browser_navigate / browser_screenshot 工具时自动显示。
 * 展示最新截图与当前 URL，支持最小化/关闭。
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Globe, Minimize2, Maximize2, X, ExternalLink } from 'lucide-react'
import { cn } from '@/lib/utils'
import { sessionBrowserPreviewMapAtom } from '@/atoms/agent-atoms'

interface BrowserPreviewOverlayProps {
  sessionId: string
}

export function BrowserPreviewOverlay({ sessionId }: BrowserPreviewOverlayProps): React.ReactElement | null {
  const [previewMap, setPreviewMap] = useAtom(sessionBrowserPreviewMapAtom)
  const preview = previewMap.get(sessionId)

  if (!preview?.visible) return null

  const { url, screenshotData, minimized } = preview

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

  return (
    <div
      className={cn(
        'absolute top-3 right-3 z-20',
        'flex flex-col rounded-xl overflow-hidden',
        'shadow-[0_8px_32px_rgba(0,0,0,0.18)] border border-border/60',
        'bg-popover backdrop-blur-sm',
        'transition-all duration-200 ease-out',
        minimized ? 'w-[220px]' : 'w-[280px]',
      )}
    >
      {/* URL 栏 */}
      <div className="flex items-center gap-1.5 px-2.5 py-2 border-b border-border/40 bg-muted/40">
        {/* 运行中指示点 */}
        <span className="relative flex h-2 w-2 shrink-0">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-60" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500" />
        </span>

        {/* URL / hostname */}
        <Globe size={11} className="shrink-0 text-muted-foreground" />
        <span className="flex-1 truncate text-[11px] text-muted-foreground leading-none select-none">
          {hostname ?? '浏览器运行中'}
        </span>

        {/* 外部链接 */}
        {url && (
          <button
            onClick={() => window.open(url, '_blank')}
            className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
            title="在系统浏览器中打开"
          >
            <ExternalLink size={11} />
          </button>
        )}

        {/* 最小化 / 展开 */}
        <button
          onClick={() => update({ minimized: !minimized })}
          className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
          title={minimized ? '展开' : '最小化'}
        >
          {minimized ? <Maximize2 size={11} /> : <Minimize2 size={11} />}
        </button>

        {/* 关闭 */}
        <button
          onClick={() => update({ visible: false })}
          className="p-0.5 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors"
          title="关闭预览"
        >
          <X size={11} />
        </button>
      </div>

      {/* 截图区域 */}
      {!minimized && (
        <div className="relative bg-muted/20" style={{ aspectRatio: '16/10' }}>
          {screenshotData ? (
            <img
              src={`data:image/png;base64,${screenshotData}`}
              alt="浏览器截图"
              className="w-full h-full object-cover object-top"
              draggable={false}
            />
          ) : (
            <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
              <Globe size={22} className="opacity-30" />
              <span className="text-[11px] opacity-50">等待截图...</span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

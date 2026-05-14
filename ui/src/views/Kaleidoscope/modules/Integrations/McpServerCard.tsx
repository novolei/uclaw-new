/**
 * McpServerCard — 集成模块的富卡片(变体 B)。
 *
 * icon + 名称 + 状态点 + 「transport · N 工具」+ 工具名 chips 预览。
 * 纯展示;点击由父级处理。状态点用功能语义色(绿/红/灰),非装饰色。
 */
import * as React from 'react'
import { cn } from '@/lib/utils'
import type { McpServerInfo } from '@/lib/types'

const STATUS_DOT: Record<string, string> = {
  connected: 'bg-emerald-500',
  error: 'bg-red-500',
  connecting: 'bg-amber-500',
  disconnected: 'bg-muted-foreground/40',
}

export interface McpServerCardProps {
  server: McpServerInfo
  toolNames: string[]
  selected: boolean
  onClick: () => void
}

export function McpServerCard({ server, toolNames, selected, onClick }: McpServerCardProps): React.ReactElement {
  const dot = STATUS_DOT[server.status] ?? STATUS_DOT.disconnected
  const isError = server.status === 'error'
  const previewChips = toolNames.slice(0, 3)
  const moreCount = toolNames.length - previewChips.length

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'rounded-xl border p-3.5 text-left transition-colors',
        selected
          ? 'border-accent/35 bg-accent/15'
          : 'border-border bg-card hover:bg-muted/40',
      )}
    >
      <div className="flex items-center gap-2">
        <div className="flex size-7 items-center justify-center rounded-lg bg-muted text-[13px]">
          {server.name.charAt(0).toUpperCase()}
        </div>
        <div className="text-[13px] font-semibold text-foreground truncate">{server.name}</div>
        <span className={cn('ml-auto size-1.5 rounded-full', dot)} title={server.status} />
      </div>
      {isError ? (
        <div className="mt-2 text-[11px] text-red-500">连接失败 · 点击查看详情</div>
      ) : (
        <div className="mt-2 text-[11px] text-muted-foreground">
          {server.transportType} · {toolNames.length} 个工具
        </div>
      )}
      {previewChips.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1">
          {previewChips.map((t, i) => (
            <span key={`${t}-${i}`} className="rounded bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground">
              {t}
            </span>
          ))}
          {moreCount > 0 && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground">
              +{moreCount}
            </span>
          )}
        </div>
      )}
    </button>
  )
}

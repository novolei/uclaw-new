import * as React from 'react'
import { cn } from '@/lib/utils'
import type { PluginInfo } from '@/lib/types'

export interface PluginCardProps {
  plugin: PluginInfo
  selected: boolean
  onClick: () => void
}

export function PluginCard({ plugin, selected, onClick }: PluginCardProps): React.ReactElement {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'rounded-xl border p-3.5 text-left transition-colors',
        selected ? 'border-accent/35 bg-accent/15' : 'border-border bg-card hover:bg-muted/40',
      )}
    >
      <div className="flex items-center gap-2">
        <div className="flex size-7 items-center justify-center rounded-lg bg-muted text-[13px]">
          {plugin.display_name.charAt(0).toUpperCase()}
        </div>
        <div className="text-[13px] font-semibold text-foreground truncate">{plugin.display_name}</div>
        <span
          className={cn('ml-auto size-1.5 rounded-full', plugin.mcp_connected ? 'bg-emerald-500' : 'bg-muted-foreground/40')}
          title={plugin.mcp_connected ? 'MCP 已连接' : 'MCP 未连接'}
        />
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground">
        v{plugin.version} · {plugin.enabled ? '已启用' : '已停用'}
      </div>
    </button>
  )
}

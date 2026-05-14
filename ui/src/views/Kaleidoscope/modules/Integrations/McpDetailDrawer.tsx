/**
 * McpDetailDrawer — 集成模块的详情抽屉(右侧 Sheet)。
 *
 * 展示选中 MCP server 的工具列表、状态/错误、操作(重启/移除/编辑/启用开关)。
 * 不做时间戳日志流 —— error 状态显示 errorMessage 即可(spec §6.3)。
 */
import * as React from 'react'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import type { McpServerInfo } from '@/lib/types'

export interface McpDetailDrawerProps {
  server: McpServerInfo | null
  toolNames: string[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onToggleEnabled: (server: McpServerInfo, next: boolean) => void
  onRestart: (server: McpServerInfo) => void
  onRemove: (server: McpServerInfo) => void
  onEdit: (server: McpServerInfo) => void
}

export function McpDetailDrawer({
  server,
  toolNames,
  open,
  onOpenChange,
  onToggleEnabled,
  onRestart,
  onRemove,
  onEdit,
}: McpDetailDrawerProps): React.ReactElement {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-[340px] sm:max-w-[340px] bg-popover">
        {server && (
          <>
            <SheetHeader>
              <SheetTitle className="flex items-center gap-2">
                <span className="truncate">{server.name}</span>
                <span className="text-[11px] font-normal text-muted-foreground">
                  {server.transportType}
                </span>
              </SheetTitle>
            </SheetHeader>

            <div className="mt-4 flex items-center justify-between">
              <span className="text-[11px] text-muted-foreground">Agent 可调用</span>
              <Switch
                checked={server.enabled}
                onCheckedChange={(next) => onToggleEnabled(server, next)}
                aria-label="启用"
              />
            </div>

            {server.status === 'error' && server.errorMessage && (
              <div className="mt-4">
                <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                  最近错误
                </div>
                <pre className="mt-1 whitespace-pre-wrap break-words rounded-md bg-destructive/10 p-2 text-[11px] text-destructive">
                  {server.errorMessage}
                </pre>
              </div>
            )}

            <div className="mt-4">
              <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                工具（{toolNames.length}）
              </div>
              <div className="mt-1.5 flex flex-col gap-1">
                {toolNames.length === 0 ? (
                  <div className="text-[11px] text-muted-foreground">
                    {server.status === 'connected' ? '此 server 未暴露工具' : '连接后显示工具'}
                  </div>
                ) : (
                  toolNames.map((t, i) => (
                    <div key={`${t}-${i}`} className="rounded bg-muted px-2 py-1 text-[11px] text-foreground">
                      {t}
                    </div>
                  ))
                )}
              </div>
            </div>

            <div className="mt-5 flex gap-2">
              <Button size="sm" variant="outline" className="flex-1" onClick={() => onEdit(server)}>
                编辑
              </Button>
              <Button size="sm" variant="outline" className="flex-1" onClick={() => onRestart(server)}>
                重启
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="flex-1 text-destructive hover:text-destructive"
                onClick={() => onRemove(server)}
              >
                移除
              </Button>
            </div>
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

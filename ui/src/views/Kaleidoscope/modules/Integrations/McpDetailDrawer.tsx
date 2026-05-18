/**
 * McpDetailDrawer — 集成模块的详情抽屉(右侧 Sheet)。
 *
 * 展示选中 MCP server 的工具列表、状态/错误、操作(重启/移除/编辑/启用开关)。
 * 不做时间戳日志流 —— error 状态显示 errorMessage 即可(spec §6.3)。
 */
import * as React from 'react'
import { toast } from 'sonner'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import type { McpServerInfo } from '@/lib/types'
import {
  refreshMcpTools,
  pingMcpServer,
  disconnectMcpServer,
} from '@/lib/tauri-bridge'

export interface McpDetailDrawerProps {
  server: McpServerInfo | null
  toolNames: string[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onToggleEnabled: (server: McpServerInfo, next: boolean) => void
  onRestart: (server: McpServerInfo) => void
  onRemove: (server: McpServerInfo) => void
  onEdit: (server: McpServerInfo) => void
  /** Sprint MCP PR-2 — invalidate parent's tools query after a refresh
   *  so the new tool list shows up without a full module-level
   *  refetch. Optional; omitting it falls back to a toast-only flow. */
  onToolsRefreshed?: () => void
  /** Sprint MCP PR-2 — same idea after disconnect, so the parent can
   *  flip the status badge without a polling round-trip. */
  onDisconnected?: () => void
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
  onToolsRefreshed,
  onDisconnected,
}: McpDetailDrawerProps): React.ReactElement {
  // PR-2 — busy flags drive disabled state + spinner. Three independent
  // operations so the user can e.g. ping while waiting for a slow
  // refresh to come back.
  const [refreshing, setRefreshing] = React.useState(false)
  const [pinging, setPinging] = React.useState(false)
  const [disconnecting, setDisconnecting] = React.useState(false)

  const handleRefreshTools = async (s: McpServerInfo) => {
    setRefreshing(true)
    try {
      const tools = await refreshMcpTools(s.id)
      const n = Array.isArray(tools) ? tools.length : 0
      toast.success(`已刷新 — ${n} 个工具`)
      onToolsRefreshed?.()
    } catch (e) {
      toast.error(`刷新失败: ${String(e)}`)
    } finally {
      setRefreshing(false)
    }
  }

  const handlePing = async (s: McpServerInfo) => {
    setPinging(true)
    try {
      const ms = await pingMcpServer(s.id)
      toast.success(`Ping ✓ ${ms}ms`)
    } catch (e) {
      toast.error(`Ping 失败: ${String(e)}`)
    } finally {
      setPinging(false)
    }
  }

  const handleDisconnect = async (s: McpServerInfo) => {
    setDisconnecting(true)
    try {
      await disconnectMcpServer(s.id)
      toast.success('已断开连接')
      onDisconnected?.()
    } catch (e) {
      toast.error(`断开失败: ${String(e)}`)
    } finally {
      setDisconnecting(false)
    }
  }
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

            <div className="mt-5 flex flex-wrap gap-2">
              <Button size="sm" variant="outline" className="flex-1 min-w-[60px]" onClick={() => onEdit(server)}>
                编辑
              </Button>
              <Button size="sm" variant="outline" className="flex-1 min-w-[60px]" onClick={() => onRestart(server)}>
                重启
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="flex-1 min-w-[60px] text-destructive hover:text-destructive"
                onClick={() => onRemove(server)}
              >
                移除
              </Button>
            </div>
            {/* PR-2 — connection-management secondary row. Only useful
                when there's an actual transport to talk to, so we hide
                refresh/ping/disconnect on disconnected servers (the
                primary affordance there is "重启" up top). */}
            {server.status === 'connected' && (
              <div className="mt-2 flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  className="flex-1 min-w-[60px] text-xs"
                  onClick={() => void handleRefreshTools(server)}
                  disabled={refreshing}
                  title="重新拉取该 server 的 tools/list — server 中途新增的工具可以这样发现"
                >
                  {refreshing ? '刷新中…' : '刷新工具'}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="flex-1 min-w-[60px] text-xs"
                  onClick={() => void handlePing(server)}
                  disabled={pinging}
                  title="JSON-RPC ping — 测试连接是否还活着，不重启"
                >
                  {pinging ? 'Ping 中…' : '测试连接'}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  className="flex-1 min-w-[60px] text-xs"
                  onClick={() => void handleDisconnect(server)}
                  disabled={disconnecting}
                  title="断开但保留配置 — 想完全删除请用「移除」"
                >
                  {disconnecting ? '断开中…' : '断开'}
                </Button>
              </div>
            )}
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

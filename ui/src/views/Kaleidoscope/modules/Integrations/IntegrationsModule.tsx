/**
 * IntegrationsModule — 万花筒「集成」模块。
 *
 * MCP server 富卡片网格 + 详情抽屉(Sheet)+ 可视化编辑器(Dialog)+ 模板库。
 * 数据:listMcpServers + listMcpTools(后者按 serverId 分组)。
 */
import * as React from 'react'
import { toast } from 'sonner'
import {
  listMcpServers,
  listMcpTools,
  toggleMcpServer,
  restartMcpServer,
  removeMcpServer,
} from '@/lib/tauri-bridge'
import type { McpServerInfo, McpServerInput } from '@/lib/types'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { Button } from '@/components/ui/button'
import { McpServerCard } from './McpServerCard'
import { McpDetailDrawer } from './McpDetailDrawer'
import { McpTemplateLibrary } from './McpTemplateLibrary'
import { McpEditorModal, type McpEditorTarget } from './McpEditorModal'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

interface McpToolRow {
  serverId: string
  name: string
}

export function IntegrationsModule(): React.ReactElement {
  const [servers, setServers] = React.useState<McpServerInfo[]>([])
  const [tools, setTools] = React.useState<McpToolRow[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [drawerOpen, setDrawerOpen] = React.useState(false)
  const [templateOpen, setTemplateOpen] = React.useState(false)
  const [editorTarget, setEditorTarget] = React.useState<McpEditorTarget | null>(null)
  const [pendingRemove, setPendingRemove] = React.useState<McpServerInfo | null>(null)
  const initialized = React.useRef(false)

  const refetch = React.useCallback(async () => {
    if (!initialized.current) setLoading(true)
    const [s, t] = await Promise.allSettled([listMcpServers(), listMcpTools()])
    if (s.status === 'fulfilled') setServers(s.value)
    else toast.error('加载 MCP server 失败', { description: String(s.reason) })
    if (t.status === 'fulfilled') {
      setTools(
        (t.value as Array<{ serverId?: string; name?: string }>).map((row) => ({
          serverId: row.serverId ?? '',
          name: row.name ?? '',
        })),
      )
    }
    initialized.current = true
    setLoading(false)
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  const toolsByServer = React.useMemo(() => {
    const map = new Map<string, string[]>()
    for (const row of tools) {
      const arr = map.get(row.serverId) ?? []
      arr.push(row.name)
      map.set(row.serverId, arr)
    }
    return map
  }, [tools])

  const selected = servers.find((s) => s.id === selectedId) ?? null
  const connectedCount = servers.filter((s) => s.status === 'connected').length

  const openCard = (server: McpServerInfo) => {
    setSelectedId(server.id)
    setDrawerOpen(true)
  }

  const onToggleEnabled = async (server: McpServerInfo, next: boolean) => {
    setServers((prev) => prev.map((s) => (s.id === server.id ? { ...s, enabled: next } : s)))
    try {
      await toggleMcpServer(server.id, next)
    } catch (err) {
      toast.error('切换状态失败', { description: String(err) })
      setServers((prev) => prev.map((s) => (s.id === server.id ? { ...s, enabled: !next } : s)))
    }
  }

  const onRestart = async (server: McpServerInfo) => {
    try {
      await restartMcpServer(server.id)
      toast.success(`已重启「${server.name}」`)
      await refetch()
    } catch (err) {
      toast.error('重启失败', { description: String(err) })
    }
  }

  const onConfirmRemove = async () => {
    if (!pendingRemove) return
    const target = pendingRemove
    setPendingRemove(null)
    try {
      await removeMcpServer(target.id)
      toast.success(`已移除「${target.name}」`)
      setDrawerOpen(false)
      setSelectedId(null)
      await refetch()
    } catch (err) {
      toast.error('移除失败', { description: String(err) })
    }
  }

  const onPickTemplate = (prefill: McpServerInput) => {
    setTemplateOpen(false)
    setEditorTarget({ mode: 'add', prefill })
  }

  const onEdit = (server: McpServerInfo) => {
    setDrawerOpen(false)
    setEditorTarget({ mode: 'edit', server })
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader
        group="capability"
        title="集成 · MCP"
        subtitle={
          loading
            ? '加载中…'
            : `${servers.length} 个 MCP server · ${connectedCount} 个已连接`
        }
        actions={
          <Button size="sm" onClick={() => setTemplateOpen(true)}>
            + 添加集成
          </Button>
        }
      />

      <div className="flex-1 min-h-0 overflow-y-auto px-8 pb-8">
        {!loading && servers.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="rounded-lg border border-dashed border-border bg-muted/10 px-8 py-10 text-center">
              <div className="text-[13px] text-foreground/80">还没有集成</div>
              <div className="mt-1 text-[11.5px] text-muted-foreground">
                点「+ 添加集成」，让 Agent 接入 Slack / GitHub / Notion。
              </div>
            </div>
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-3">
            {servers.map((server) => (
              <McpServerCard
                key={server.id}
                server={server}
                toolNames={toolsByServer.get(server.id) ?? []}
                selected={server.id === selectedId && drawerOpen}
                onClick={() => openCard(server)}
              />
            ))}
            <button
              type="button"
              onClick={() => setTemplateOpen(true)}
              className="flex min-h-[88px] items-center justify-center rounded-xl border border-dashed border-border text-[12px] text-muted-foreground hover:bg-muted/40"
            >
              + 从模板添加
            </button>
          </div>
        )}
      </div>

      <McpDetailDrawer
        server={selected}
        toolNames={selected ? toolsByServer.get(selected.id) ?? [] : []}
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
        onRestart={(s) => void onRestart(s)}
        onRemove={(s) => setPendingRemove(s)}
        onEdit={onEdit}
      />

      <McpTemplateLibrary
        open={templateOpen}
        onOpenChange={setTemplateOpen}
        onPick={onPickTemplate}
      />

      <McpEditorModal
        target={editorTarget}
        onOpenChange={(open) => !open && setEditorTarget(null)}
        onSaved={() => void refetch()}
      />

      <AlertDialog
        open={pendingRemove !== null}
        onOpenChange={(open) => {
          if (!open) setPendingRemove(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>移除集成？</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-2 text-sm text-muted-foreground">
                <p>即将移除 MCP server「{pendingRemove?.name ?? ''}」，它的配置会从 mcp_servers.json 删除。</p>
                <p>你之后可以重新添加它。</p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void onConfirmRemove()}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              移除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

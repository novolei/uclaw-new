import * as React from 'react'
import { toast } from 'sonner'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Switch } from '@/components/ui/switch'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { getPluginDetail, getPluginEnv, setPluginEnv, deletePluginEnv } from '@/lib/tauri-bridge'
import type { PluginInfo, PluginDetail } from '@/lib/types'

function ChipList({ label, items }: { label: string; items?: string[] }): React.ReactElement {
  const list = items ?? []
  return (
    <div className="mt-4">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {label}（{list.length}）
      </div>
      <div className="mt-1.5 flex flex-wrap gap-1">
        {list.length === 0 ? (
          <span className="text-[11px] text-muted-foreground">无</span>
        ) : (
          list.map((t, i) => (
            <span key={`${t}-${i}`} className="rounded bg-muted px-2 py-1 text-[11px] text-foreground">{t}</span>
          ))
        )}
      </div>
    </div>
  )
}

function PermBadge({ label, on }: { label: string; on: boolean }): React.ReactElement {
  return (
    <span className={cn(
      'rounded px-1.5 py-0.5 text-[10px]',
      on ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400' : 'bg-muted text-muted-foreground',
    )}>
      {label}{on ? '' : '（无）'}
    </span>
  )
}

export interface PluginDetailDrawerProps {
  plugin: PluginInfo | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onToggleEnabled: (plugin: PluginInfo, next: boolean) => void
  onUninstall?: (plugin: PluginInfo) => void
  onUpgrade?: (plugin: PluginInfo) => void
}

export function PluginDetailDrawer({ plugin, open, onOpenChange, onToggleEnabled, onUninstall, onUpgrade }: PluginDetailDrawerProps): React.ReactElement {
  const [detail, setDetail] = React.useState<PluginDetail | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [env, setEnv] = React.useState<Record<string, string>>({})
  const [newKey, setNewKey] = React.useState('')
  const [newVal, setNewVal] = React.useState('')

  React.useEffect(() => {
    if (!open || !plugin) return
    let active = true
    setLoading(true)
    setDetail(null)
    setEnv({})
    getPluginDetail(plugin.id)
      .then((d) => { if (active) setDetail(d) })
      .catch((e) => { if (active) toast.error('加载插件详情失败', { description: String(e) }) })
      .finally(() => { if (active) setLoading(false) })
    getPluginEnv(plugin.id).then((e) => { if (active) setEnv(e) }).catch(() => {})
    return () => { active = false }
  }, [open, plugin])

  const refreshEnv = () => {
    if (plugin) getPluginEnv(plugin.id).then(setEnv).catch(() => {})
  }

  const addEnv = async () => {
    if (!plugin || !newKey.trim()) return
    await setPluginEnv(plugin.id, newKey.trim(), newVal)
    setNewKey('')
    setNewVal('')
    refreshEnv()
    toast.success('已保存，重启生效')
  }

  const removeEnv = async (k: string) => {
    if (!plugin) return
    await deletePluginEnv(plugin.id, k)
    refreshEnv()
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-[360px] sm:max-w-[360px] bg-popover overflow-y-auto">
        {plugin && (
          <>
            <SheetHeader>
              <SheetTitle className="flex items-center gap-2">
                <span className="truncate">{plugin.display_name}</span>
                <span className="text-[11px] font-normal text-muted-foreground">v{plugin.version}</span>
              </SheetTitle>
            </SheetHeader>

            <div className="mt-4 flex items-center justify-between">
              <span className="text-[11px] text-muted-foreground">启用（重启后生效）</span>
              <Switch checked={plugin.enabled} onCheckedChange={(next) => onToggleEnabled(plugin, next)} aria-label="启用" />
            </div>

            <div className="mt-5 flex flex-wrap gap-2">
              <Button size="sm" variant="outline" className="flex-1 min-w-[60px]" onClick={() => onUpgrade?.(plugin)}>
                更新
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="flex-1 min-w-[60px] text-destructive hover:text-destructive"
                onClick={() => onUninstall?.(plugin)}
              >
                卸载
              </Button>
            </div>

            {loading || !detail ? (
              <div className="mt-6 text-[11px] text-muted-foreground">{loading ? '加载详情中…' : '无详情'}</div>
            ) : (
              <>
                {detail.description && (
                  <div className="mt-3 text-[11px] text-muted-foreground">{detail.description}</div>
                )}
                <div className="mt-1 text-[10px] text-muted-foreground/70">作者：{detail.author_name}</div>

                <ChipList label="工具" items={detail.contributes.tools} />
                <ChipList label="技能" items={detail.contributes.skills} />
                <ChipList label="命令" items={detail.contributes.commands} />
                <ChipList label="MCP 服务" items={detail.contributes.mcp_servers} />

                <div className="mt-4">
                  <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">权限 / 沙箱</div>
                  <div className="mt-1.5 flex flex-wrap gap-1.5">
                    <PermBadge label="网络" on={detail.permissions.network} />
                    <PermBadge label="读文件" on={detail.permissions.filesystem_read} />
                    <PermBadge label="写文件" on={detail.permissions.filesystem_write} />
                    <PermBadge label="子进程" on={detail.permissions.run_subprocess} />
                  </div>
                </div>

                <div className="mt-4">
                  <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">环境变量</div>
                  <div className="mt-0.5 text-[10px] text-muted-foreground/70">API key 等；重启后生效</div>
                  {Object.entries(env).map(([k, v]) => (
                    <div key={k} className="mt-1.5 flex items-center gap-1.5">
                      <span className="text-[11px] font-mono text-foreground truncate flex-1">{k}</span>
                      <span className="text-[11px] text-muted-foreground truncate flex-1">{v ? '••••••' : ''}</span>
                      <Button size="sm" variant="outline" className="text-destructive" onClick={() => void removeEnv(k)}>删除</Button>
                    </div>
                  ))}
                  <div className="mt-2 flex items-center gap-1.5">
                    <Input
                      className="h-7 text-[11px]"
                      placeholder="KEY"
                      value={newKey}
                      onChange={(e) => setNewKey(e.target.value)}
                    />
                    <Input
                      className="h-7 text-[11px]"
                      placeholder="value"
                      value={newVal}
                      onChange={(e) => setNewVal(e.target.value)}
                    />
                    <Button size="sm" variant="outline" onClick={() => void addEnv()} disabled={!newKey.trim()}>添加</Button>
                  </div>
                </div>

                {detail.preflight && detail.preflight.verdict !== 'pass' && detail.preflight.findings.length > 0 && (
                  <div className="mt-4">
                    <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">预检</div>
                    <div className="mt-1.5 flex flex-col gap-1">
                      {detail.preflight.findings.map((f, i) => (
                        <div
                          key={i}
                          className={cn(
                            'rounded px-2 py-1 text-[11px]',
                            f.severity === 'error'
                              ? 'bg-destructive/10 text-destructive'
                              : f.severity === 'warning'
                                ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
                                : 'bg-muted text-muted-foreground',
                          )}
                        >
                          {f.message}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

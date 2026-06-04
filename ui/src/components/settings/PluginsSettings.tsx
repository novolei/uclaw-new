import { useState, useEffect, useCallback } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { Switch } from '@/components/ui/switch'
import { listPlugins, setPluginEnabled } from '@/lib/tauri-bridge'
import type { PluginInfo } from '@/lib/types'
import { cn } from '@/lib/utils'
import { toast } from 'sonner'

export function PluginsSettings() {
  const [plugins, setPlugins] = useState<PluginInfo[] | null>(null)
  const [loading, setLoading] = useState(false)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      setPlugins(await listPlugins())
    } catch (e) {
      toast.error('加载插件失败', { description: String(e) })
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void refresh() }, [refresh])

  const onToggle = async (p: PluginInfo, next: boolean) => {
    setPlugins((prev) => prev?.map((x) => (x.id === p.id ? { ...x, enabled: next } : x)) ?? prev)
    try {
      await setPluginEnabled(p.id, next)
    } catch (e) {
      toast.error('切换插件状态失败', { description: String(e) })
      setPlugins((prev) => prev?.map((x) => (x.id === p.id ? { ...x, enabled: !next } : x)) ?? prev)
    }
  }

  return (
    <SettingsSection
      title="插件"
      description="管理已安装的插件（启用 / 停用在下次会话或重连后生效）"
    >
      <SettingsCard>
        {plugins == null ? (
          <div className="px-4 py-3.5 text-sm text-muted-foreground">加载中…</div>
        ) : plugins.length === 0 ? (
          <div className="px-4 py-3.5 text-sm text-muted-foreground">未安装插件</div>
        ) : (
          plugins.map((p) => (
            <SettingsRow key={p.id} label={p.display_name} description={`v${p.version}`}>
              <div className="flex items-center gap-3">
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span
                    className={cn(
                      'size-2 rounded-full flex-shrink-0',
                      p.mcp_connected ? 'bg-emerald-500' : 'bg-muted-foreground/40',
                    )}
                  />
                  {p.mcp_connected ? 'MCP 已连接' : 'MCP 未连接'}
                </span>
                <Switch
                  checked={p.enabled}
                  onCheckedChange={(next) => onToggle(p, next)}
                  disabled={loading}
                  aria-label="启用"
                />
              </div>
            </SettingsRow>
          ))
        )}
      </SettingsCard>
    </SettingsSection>
  )
}

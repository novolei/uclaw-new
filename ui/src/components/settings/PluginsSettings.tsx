import { useState, useEffect, useCallback } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { listPlugins, setPluginEnabled, installPluginFromGit, installPluginFromDir } from '@/lib/tauri-bridge'
import type { PluginInfo } from '@/lib/types'
import { toast } from 'sonner'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { PluginCard } from './PluginCard'
import { PluginDetailDrawer } from './PluginDetailDrawer'

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

  const [gitUrl, setGitUrl] = useState('')
  const [installing, setInstalling] = useState(false)

  const doInstall = async (fn: () => Promise<{ display_name: string }>) => {
    setInstalling(true)
    try {
      const info = await fn()
      toast.success(`已安装 ${info.display_name}`, { description: '重启应用以激活其工具 / 命令' })
      setGitUrl('')
      await refresh()
    } catch (e) {
      toast.error('安装失败', { description: String(e) })
    } finally {
      setInstalling(false)
    }
  }
  const onInstallGit = () => {
    const url = gitUrl.trim()
    if (!url) return
    void doInstall(() => installPluginFromGit(url))
  }
  const onInstallDir = async () => {
    const dir = await openDialog({ directory: true, multiple: false })
    if (!dir || typeof dir !== 'string') return
    void doInstall(() => installPluginFromDir(dir))
  }

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [drawerOpen, setDrawerOpen] = useState(false)
  const selected = plugins?.find((p) => p.id === selectedId) ?? null

  return (
    <>
      <SettingsSection title="安装插件" description="从 git 仓库或本地文件夹安装（重启后激活）">
        <SettingsCard>
          <div className="flex items-center gap-2 px-4 py-3.5">
            <Input
              value={gitUrl}
              onChange={(e) => setGitUrl(e.target.value)}
              placeholder="git 仓库 URL，例如 https://github.com/user/plugin.git"
              disabled={installing}
              onKeyDown={(e) => { if (e.key === 'Enter') onInstallGit() }}
            />
            <Button onClick={onInstallGit} disabled={installing || !gitUrl.trim()}>安装</Button>
            <Button variant="outline" onClick={onInstallDir} disabled={installing}>从文件夹导入</Button>
          </div>
        </SettingsCard>
      </SettingsSection>
      <SettingsSection
        title="插件"
        description="管理已安装的插件（启用 / 停用在下次会话或重连后生效）"
      >
        {plugins == null ? (
          <SettingsCard><div className="px-4 py-3.5 text-sm text-muted-foreground">加载中…</div></SettingsCard>
        ) : plugins.length === 0 ? (
          <SettingsCard><div className="px-4 py-3.5 text-sm text-muted-foreground">未安装插件</div></SettingsCard>
        ) : (
          <div className="grid grid-cols-2 gap-3">
            {plugins.map((p) => (
              <PluginCard
                key={p.id}
                plugin={p}
                selected={p.id === selectedId && drawerOpen}
                onClick={() => { setSelectedId(p.id); setDrawerOpen(true) }}
              />
            ))}
          </div>
        )}
      </SettingsSection>
      <PluginDetailDrawer
        plugin={selected}
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        onToggleEnabled={(p, next) => void onToggle(p, next)}
      />
    </>
  )
}

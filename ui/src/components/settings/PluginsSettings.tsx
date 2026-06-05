import { useState, useEffect, useCallback } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { listPlugins, setPluginEnabled, installPluginFromGit, installPluginFromDir, listCatalog, installPluginFromCatalog, searchRegistry, installPluginFromRegistry, uninstallPlugin, upgradePlugin } from '@/lib/tauri-bridge'
import type { PluginInfo, CatalogEntry, RegistryEntry } from '@/lib/types'
import { toast } from 'sonner'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { PluginCard } from './PluginCard'
import { PluginDetailDrawer } from './PluginDetailDrawer'

export function PluginsSettings() {
  const [plugins, setPlugins] = useState<PluginInfo[] | null>(null)
  const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null)
  const [loading, setLoading] = useState(false)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const [ps, cat] = await Promise.all([listPlugins(), listCatalog()])
      setPlugins(ps)
      setCatalog(cat)
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

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [drawerOpen, setDrawerOpen] = useState(false)
  const selected = plugins?.find((p) => p.id === selectedId) ?? null

  const onUninstall = async (p: PluginInfo) => {
    if (!window.confirm(`确定卸载 ${p.display_name}？`)) return
    try {
      await uninstallPlugin(p.id)
      toast.success(`已卸载 ${p.display_name}`)
      setDrawerOpen(false)
      await refresh()
    } catch (e) {
      toast.error('卸载失败', { description: String(e) })
    }
  }
  const onUpgrade = async (p: PluginInfo) => {
    try {
      const info = await upgradePlugin(p.id)
      toast.success(`已更新 ${info.display_name}`, { description: '重启应用以激活新版本' })
      await refresh()
    } catch (e) {
      toast.error('更新失败', { description: String(e) })
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

  const onInstallCatalog = (e: CatalogEntry) =>
    doInstall(async () => {
      const info = await installPluginFromCatalog(e.slug)
      if (e.setup_note) toast.info('需要额外配置', { description: e.setup_note })
      return info
    })

  const [regQuery, setRegQuery] = useState('')
  const [regResults, setRegResults] = useState<RegistryEntry[] | null>(null)
  const [regSearching, setRegSearching] = useState(false)

  const doRegSearch = async () => {
    setRegSearching(true)
    try {
      setRegResults(await searchRegistry(regQuery.trim() || undefined))
    } catch (e) {
      toast.error('搜索失败', { description: String(e) })
      setRegResults([])
    } finally {
      setRegSearching(false)
    }
  }

  const onInstallRegistry = (e: RegistryEntry) =>
    doInstall(async () => {
      const info = await installPluginFromRegistry(e)
      if (e.setup_note) toast.info('需要额外配置', { description: e.setup_note })
      return info
    })

  const installedSlugs = new Set((plugins ?? []).map((p) => p.id))

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
      {catalog != null && catalog.length > 0 && (
        <SettingsSection title="插件市场" description="一键安装社区精选的 MCP 插件（重启后激活）">
          <div className="grid grid-cols-2 gap-3">
            {catalog.map((entry) => (
              <SettingsCard key={entry.slug}>
                <div className="flex flex-col gap-2 px-4 py-3.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-sm">{entry.name}</span>
                    <Badge variant="secondary" className="shrink-0 text-xs">{entry.category}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground line-clamp-2">{entry.description}</p>
                  {entry.setup_note && (
                    <p className="text-xs text-muted-foreground italic">需配置</p>
                  )}
                  <Button
                    size="sm"
                    disabled={installing || installedSlugs.has(entry.slug)}
                    onClick={() => void onInstallCatalog(entry)}
                  >
                    {installedSlugs.has(entry.slug) ? '已安装' : '安装'}
                  </Button>
                </div>
              </SettingsCard>
            ))}
          </div>
        </SettingsSection>
      )}
      {catalog == null && loading && (
        <SettingsSection title="插件市场" description="一键安装社区精选的 MCP 插件（重启后激活）">
          <SettingsCard><div className="px-4 py-3.5 text-sm text-muted-foreground">加载中…</div></SettingsCard>
        </SettingsSection>
      )}
      <SettingsSection title="在线市场" description="搜索官方 MCP 注册表（仅 stdio 包，约前 100 条）">
        <SettingsCard>
          <div className="flex items-center gap-2 px-4 py-3.5">
            <Input
              value={regQuery}
              onChange={(e) => setRegQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') void doRegSearch() }}
              placeholder="搜索社区 MCP 服务器…"
            />
            <Button onClick={() => void doRegSearch()} disabled={regSearching}>
              {regSearching ? '搜索中…' : '搜索'}
            </Button>
          </div>
        </SettingsCard>
        {regResults == null ? (
          <div className="px-1 py-2 text-xs text-muted-foreground">搜索官方 MCP 注册表（仅 stdio 包，约前 100 条）</div>
        ) : regResults.length === 0 ? (
          <div className="px-1 py-2 text-xs text-muted-foreground">无结果</div>
        ) : (
          <div className="grid grid-cols-2 gap-3 mt-2">
            {regResults.map((entry) => (
              <SettingsCard key={entry.id}>
                <div className="flex flex-col gap-2 px-4 py-3.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-sm">{entry.title || entry.name}</span>
                    <Badge variant="secondary" className="shrink-0 text-xs">registry</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground line-clamp-2">{entry.description}</p>
                  {entry.setup_note && (
                    <p className="text-xs text-muted-foreground italic">需配置</p>
                  )}
                  <Button
                    size="sm"
                    disabled={installing || installedSlugs.has(entry.id)}
                    onClick={() => void onInstallRegistry(entry)}
                  >
                    {installedSlugs.has(entry.id) ? '已安装' : '安装'}
                  </Button>
                </div>
              </SettingsCard>
            ))}
          </div>
        )}
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
        onUninstall={(p) => void onUninstall(p)}
        onUpgrade={(p) => void onUpgrade(p)}
      />
    </>
  )
}

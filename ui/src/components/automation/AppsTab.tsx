import * as React from 'react'
import { motion, AnimatePresence } from 'motion/react'
import { ChevronDown, Trash2, CheckCircle2, AlertCircle } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { CategoryIcon } from './category-icon'
import {
  listInstalledMarketplaceAutomations,
  uninstallMarketplaceHuman,
  checkMarketplaceUpdates,
  listStandaloneInstalls,
  type InstalledAutomation,
  type StandaloneInstall,
} from '@/lib/tauri-bridge'
import { AppTypeBadge } from './AppTypeBadge'
import { UpgradeModal } from './UpgradeModal'

export function AppsTab(): React.ReactElement {
  const [items, setItems] = React.useState<InstalledAutomation[] | null>(null)
  const [standaloneItems, setStandaloneItems] = React.useState<StandaloneInstall[]>([])
  const [expanded, setExpanded] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [updateSlugs, setUpdateSlugs] = React.useState<Set<string>>(new Set())
  const [upgradeTarget, setUpgradeTarget] = React.useState<InstalledAutomation | null>(null)

  const reload = React.useCallback(async () => {
    setLoading(true)
    try {
      const [data, standalone] = await Promise.all([
        listInstalledMarketplaceAutomations(),
        listStandaloneInstalls(),
      ])
      setItems(data)
      setStandaloneItems(standalone)
    } catch (err) {
      toast.error(`加载失败：${String(err)}`)
      setItems([])
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void reload()
  }, [reload])

  // Fetch available updates alongside the installed list
  React.useEffect(() => {
    checkMarketplaceUpdates()
      .then((updates) => setUpdateSlugs(new Set(updates.map((u) => u.slug))))
      .catch(() => {
        // Non-fatal: badge simply won't appear if update check fails
      })
  }, [])

  const handleUninstall = async (item: InstalledAutomation) => {
    if (!window.confirm(`确定卸载 ${item.name} 吗？\n会一并删除依赖的 skill 文件。`)) return
    try {
      await uninstallMarketplaceHuman(item.slug)
      toast.success(`已卸载 ${item.name}`)
      await reload()
    } catch (err) {
      toast.error(`卸载失败：${String(err)}`)
    }
  }

  // titlebar-no-drag on every root branch: AppsTab's root IS the scroll
  // container and holds all interactive content (expand/uninstall/upgrade
  // buttons) — it opts out of the window-drag region wholesale, mirroring
  // the workspace pattern. See KaleidoscopeShell.
  if (loading && items === null) {
    return <div className="titlebar-no-drag px-6 py-8 text-[12px] text-muted-foreground">加载中…</div>
  }

  if (!items || (items.length === 0 && standaloneItems.length === 0)) {
    return (
      <div className="titlebar-no-drag flex flex-col items-center justify-center px-6 py-16 text-center">
        <div className="text-[14px] text-foreground mb-2">暂无已安装的数字人</div>
        <div className="text-[12px] text-muted-foreground">
          去「应用商店」装一个，或者关闭此面板回到聊天。
        </div>
      </div>
    )
  }

  return (
    <div className="titlebar-no-drag flex flex-col h-full overflow-y-auto px-6 py-4">
      <div className="text-[11px] text-muted-foreground mb-3 leading-relaxed">
        以下是已安装数字人随附的 skill / 能力依赖，以及从商店单独安装的技能和 MCP。
      </div>
      <div className="flex flex-col gap-2">
        {items.map((item) => {
          const isOpen = expanded === item.slug
          return (
            <div
              key={item.slug}
              className="rounded-xl border border-border/50 bg-card overflow-hidden"
            >
              <div className="flex items-center gap-3 w-full px-4 py-3 hover:bg-muted/40 transition-colors text-left">
                {/* expand toggle — clicking name/icon expands the detail panel */}
                <button
                  type="button"
                  onClick={() => setExpanded(isOpen ? null : item.slug)}
                  className="flex items-center gap-3 flex-1 min-w-0 text-left"
                >
                  <div className="w-10 h-10 rounded-lg bg-primary/8 flex items-center justify-center shrink-0">
                    <CategoryIcon name={item.icon ?? item.category} size={18} className="text-primary/80" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-[14px] font-medium truncate">{item.name}</div>
                    <div className="text-[11px] text-muted-foreground tabular-nums">v{item.version}</div>
                  </div>
                </button>
                {/* uninstall is a sibling of the expand button, not a child, so clicks don't bubble to toggle */}
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation()
                    void handleUninstall(item)
                  }}
                  className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-muted-foreground hover:text-danger hover:bg-danger-bg transition-colors"
                >
                  <Trash2 size={12} />
                  卸载
                </button>
                {updateSlugs.has(item.slug) && (
                  <button
                    type="button"
                    onClick={(e) => { e.stopPropagation(); setUpgradeTarget(item) }}
                    className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-primary hover:bg-primary/10 transition-colors"
                  >
                    升级
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => setExpanded(isOpen ? null : item.slug)}
                  className="text-muted-foreground"
                  aria-label={isOpen ? '收起' : '展开'}
                >
                  <ChevronDown
                    size={14}
                    className={cn('transition-transform', isOpen && 'rotate-180')}
                  />
                </button>
              </div>
              <AnimatePresence initial={false}>
                {isOpen && (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: 'auto', opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={{ duration: 0.22, ease: [0.32, 0.72, 0, 1] }}
                    className="overflow-hidden"
                  >
                    <div className="px-4 pb-4 border-t border-border/50 pt-3">
                      {item.bundledSkills.length > 0 && (
                        <div className="mb-3">
                          <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
                            Bundled Skills
                          </div>
                          <ul className="flex flex-col gap-1.5">
                            {item.bundledSkills.map((s) => (
                              <li key={s.skillId} className="text-[12px]">
                                <span className="text-foreground">{s.skillId}</span>
                                {s.description && (
                                  <span className="text-muted-foreground"> · {s.description}</span>
                                )}
                                <div className="text-[10px] text-muted-foreground/70 mt-0.5 font-mono truncate">
                                  {s.installPath}
                                </div>
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                      {item.requiredCapabilities.length > 0 && (
                        <div>
                          <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
                            Required Capabilities
                          </div>
                          <ul className="flex flex-col gap-1.5">
                            {item.requiredCapabilities.map((c) => (
                              <li key={c.mcpId} className="flex items-center gap-2 text-[12px]">
                                {c.status === 'mapped' ? (
                                  <CheckCircle2 size={13} className="text-success" />
                                ) : (
                                  <AlertCircle size={13} className="text-warning" />
                                )}
                                <span className="text-foreground">{c.mcpId}</span>
                                {c.status === 'mapped' ? (
                                  <span className="text-[11px] text-success">· 已映射到 {c.mappedTo}</span>
                                ) : (
                                  <span className="text-[11px] text-warning">· 待 Phase 3b-γ 支持</span>
                                )}
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          )
        })}
      </div>

      {standaloneItems.length > 0 && (
        <div className="mt-4">
          <div className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
            独立技能 / MCP
          </div>
          <div className="flex flex-col gap-2">
            {standaloneItems.map((item) => (
              <div
                key={item.slug}
                className="rounded-xl border border-border/50 bg-card flex items-center gap-3 px-4 py-3"
              >
                <AppTypeBadge type={item.itemType} />
                <div className="flex-1 min-w-0">
                  <div className="text-[14px] font-medium truncate">{item.slug}</div>
                  <div className="text-[11px] text-muted-foreground tabular-nums">v{item.version}</div>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    void (async () => {
                      try {
                        await uninstallMarketplaceHuman(item.slug)
                        toast.success(`已卸载 ${item.slug}`)
                        await reload()
                      } catch (err) {
                        toast.error(`卸载失败：${String(err)}`)
                      }
                    })()
                  }}
                  className="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-muted-foreground hover:text-danger hover:bg-danger-bg transition-colors"
                >
                  <Trash2 size={12} />
                  卸载
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {upgradeTarget && (
        <UpgradeModal
          slug={upgradeTarget.slug}
          name={upgradeTarget.name}
          currentVersion={upgradeTarget.version}
          installedSkillIds={upgradeTarget.bundledSkills.map((s) => s.skillId)}
          onClose={() => setUpgradeTarget(null)}
          onUpgraded={() => { void reload() }}
        />
      )}
    </div>
  )
}

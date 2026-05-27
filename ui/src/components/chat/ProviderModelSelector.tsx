/**
 * ProviderModelSelector — 消息 Composer 中的模型选择器
 *
 * 数据来源：新 Provider 体系 (getAllConfiguredModels)，非 legacy channels。
 * 选择后同时更新：
 *   - activeProviderModelAtom (localStorage + 全局 Jotai)
 *   - backend active_model (providers.json via setActiveModel)
 *   - backend role_models['chat'] (providers.json via setRoleModel)
 */
import * as React from 'react'
import { useAtom } from 'jotai'
import { useSetAtom } from 'jotai'
import { ChevronDown, Cpu, Search, Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import { activeProviderModelAtom } from '@/atoms/active-model'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { getAllConfiguredModels, setActiveModel, setRoleModel } from '@/lib/tauri-bridge'

interface ModelGroup {
  providerId: string
  models: string[]
}

export function ProviderModelSelector() {
  const [activeModel, setActiveModelAtom] = useAtom(activeProviderModelAtom)
  const [groups, setGroups] = React.useState<ModelGroup[]>([])
  const [open, setOpen] = React.useState(false)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)
  const goToProviderSettings = () => {
    setOpen(false)
    setSettingsOpen(true)
    setSettingsTab('connectivity')
  }
  const [search, setSearch] = React.useState('')
  const containerRef = React.useRef<HTMLDivElement>(null)
  const searchRef = React.useRef<HTMLInputElement>(null)

  // Load models when popover opens
  React.useEffect(() => {
    if (!open) return
    setSearch('')
    getAllConfiguredModels()
      .then((raw) =>
        setGroups(
          raw
            .filter(([, mids]) => mids.length > 0)
            .map(([pid, mids]) => ({ providerId: pid, models: mids })),
        ),
      )
      .catch(console.error)
  }, [open])

  // Focus search when opened
  React.useEffect(() => {
    if (open) setTimeout(() => searchRef.current?.focus(), 50)
  }, [open])

  // Close on outside click / ESC
  React.useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false) }
    const onMouse = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onMouse)
    return () => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onMouse)
    }
  }, [open])

  // Filtered groups
  const filteredGroups = React.useMemo(() => {
    const q = search.toLowerCase().trim()
    if (!q) return groups
    return groups
      .map((g) => ({ ...g, models: g.models.filter((m) => m.toLowerCase().includes(q) || g.providerId.toLowerCase().includes(q)) }))
      .filter((g) => g.models.length > 0)
  }, [groups, search])

  const handleSelect = async (providerId: string, modelId: string) => {
    setOpen(false)
    // Optimistic local update
    setActiveModelAtom({ providerId, modelId })
    // Persist to backend
    try {
      await Promise.all([
        setActiveModel(providerId, modelId),
        setRoleModel('chat', `${providerId}/${modelId}`),
      ])
    } catch (e) {
      console.error('Failed to persist model selection:', e)
    }
  }

  const hasModels = groups.some((g) => g.models.length > 0)

  return (
    <div ref={containerRef} className="relative">
      {/* Trigger button */}
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
      >
        <Cpu className="size-3.5 shrink-0" />
        <span className="max-w-[180px] truncate">
          {activeModel ? `${activeModel.providerId} / ${activeModel.modelId}` : '选择模型'}
        </span>
        <ChevronDown className={cn('size-3 shrink-0 transition-transform', open && 'rotate-180')} />
      </button>

      {/* Popover */}
      {open && (
        <div className="absolute bottom-full mb-1.5 left-0 z-50 w-72 rounded-xl border border-border bg-popover shadow-xl overflow-hidden">
          {/* Search */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border/60">
            <Search className="size-3.5 text-muted-foreground/60 shrink-0" />
            <input
              ref={searchRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索模型..."
              className="flex-1 bg-transparent text-[12px] outline-none placeholder:text-muted-foreground/50"
            />
          </div>

          {/* List */}
          <div className="max-h-72 overflow-y-auto">
            {filteredGroups.length === 0 ? (
              <div className="py-6 text-center">
                <Cpu className="mx-auto mb-1.5 size-4 text-muted-foreground/30" />
                <p className="text-[11px] text-muted-foreground">
                  {hasModels ? '未找到模型' : '暂无已配置的模型'}
                </p>
                {!hasModels && (
                  <div className="mt-1 flex flex-col items-center gap-1">
                    <button
                      type="button"
                      onClick={goToProviderSettings}
                      className="rounded-md bg-primary px-2 py-1 text-[11px] font-medium text-primary-foreground hover:bg-primary/90"
                    >
                      配置服务商 →
                    </button>
                  </div>
                )}
              </div>
            ) : (
              filteredGroups.map((group) => (
                <div key={group.providerId}>
                  <div className="px-3 py-1 text-[9.5px] font-semibold uppercase tracking-widest text-muted-foreground/60 bg-muted/30 border-b border-border/40">
                    {group.providerId}
                  </div>
                  {group.models.map((mid) => {
                    const selected =
                      activeModel?.providerId === group.providerId && activeModel?.modelId === mid
                    return (
                      <button
                        key={mid}
                        type="button"
                        onClick={() => void handleSelect(group.providerId, mid)}
                        className={cn(
                          'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-accent/60',
                          selected && 'text-primary',
                        )}
                      >
                        {selected ? (
                          <Check className="size-3 shrink-0 text-primary" />
                        ) : (
                          <span className="size-3 shrink-0" />
                        )}
                        <span className={cn('truncate', selected && 'font-medium')}>{mid}</span>
                      </button>
                    )
                  })}
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  )
}

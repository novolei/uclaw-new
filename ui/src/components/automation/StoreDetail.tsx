import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { motion, AnimatePresence } from 'motion/react'
import { ArrowLeft, Download, Loader2, AlertTriangle } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { AppTypeBadge } from './AppTypeBadge'
import {
  marketplaceSelectedSlugAtom,
  marketplaceDetailAtom,
  marketplaceDetailLoadingAtom,
  marketplaceDetailSubtabAtom,
  automationsSubviewAtom,
  installWizardAtom,
  userLocaleAtom,
  type DetailSubTab,
} from '@/atoms/marketplace'
import { getMarketplaceDetail } from '@/lib/tauri-bridge'
import { InstallWizard } from './InstallWizard'
import {
  localizeEntry,
  localizeSpec,
  localizeConfig,
  localizeOption,
  type SpecI18n,
} from '@/lib/marketplace-i18n'

interface ConfigSchemaEntry {
  key: string
  label: string
  description?: string
  placeholder?: string
  type?: string
  options?: Array<{ label: string; value: string }>
  required?: boolean
  default?: unknown
}

const TABS: { id: DetailSubTab; label: string }[] = [
  { id: 'overview', label: '概览' },
  { id: 'config', label: '配置' },
  { id: 'requires', label: '依赖' },
  { id: 'prompt', label: '提示词' },
]

export function StoreDetail(): React.ReactElement {
  const slug = useAtomValue(marketplaceSelectedSlugAtom)
  const [detail, setDetail] = useAtom(marketplaceDetailAtom)
  const [loading, setLoading] = useAtom(marketplaceDetailLoadingAtom)
  const [activeTab, setActiveTab] = useAtom(marketplaceDetailSubtabAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)
  const setWizard = useSetAtom(installWizardAtom)
  const [promptExpanded, setPromptExpanded] = React.useState(false)
  const locale = useAtomValue(userLocaleAtom)

  // Load detail when slug changes
  React.useEffect(() => {
    if (!slug) return
    setLoading(true)
    setActiveTab('overview')
    getMarketplaceDetail(slug)
      .then(setDetail)
      .catch((err) => {
        toast.error(`加载详情失败：${String(err)}`)
        setSubview('store')
      })
      .finally(() => setLoading(false))
  }, [slug, setDetail, setLoading, setActiveTab, setSubview])

  // Esc returns to store grid
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSubview('store')
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [setSubview])

  if (loading || !detail) {
    return (
      <div className="flex items-center gap-2 justify-center py-16 text-muted-foreground">
        <Loader2 size={14} className="animate-spin" />
        <span className="text-[13px]">正在加载详情...</span>
      </div>
    )
  }

  const { item, parsedSpecJson, requiresMcps, requiresSkills, installedVersion, specYaml } = detail
  const isInstalled = installedVersion !== null
  const hasUpdate = isInstalled && installedVersion !== item.version

  // Narrow parsedSpecJson cautiously — only access what's needed.
  const spec = (parsedSpecJson ?? null) as { i18n?: SpecI18n; config_schema?: ConfigSchemaEntry[] } | null
  const specI18n = spec?.i18n

  // Spec-level overlay wins; entry-level is the fallback.
  const displayName =
    localizeSpec('name', null, specI18n, locale) ||
    localizeEntry('name', item.name, item.i18n, locale)
  const displayDesc =
    localizeSpec('description', null, specI18n, locale) ||
    localizeEntry('description', item.description, item.i18n, locale)

  const openInstallWizard = () => {
    setWizard({
      step: 'scope',
      slug: item.slug,
      spaceId: null,
      userConfig: {},
      progress: null,
      error: null,
    })
  }

  return (
    <div className="relative flex flex-col h-full overflow-hidden">
      {/* Sticky header */}
      <div className="sticky top-0 z-10 backdrop-blur-md bg-content-area/95 border-b border-border/50">
        <div className="flex items-center gap-3 px-6 py-3">
          <button
            type="button"
            onClick={() => setSubview('store')}
            className="text-muted-foreground hover:text-foreground transition-colors"
            title="返回市场 (Esc)"
          >
            <ArrowLeft size={16} />
          </button>
          <div className="w-9 h-9 rounded-md bg-primary/10 flex items-center justify-center text-[14px]">
            {item.icon ?? '🤖'}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-[15px] font-semibold truncate">{displayName}</span>
              <AppTypeBadge type={item.appType} />
              <span className="text-[11px] text-muted-foreground tabular-nums">v{item.version}</span>
              {hasUpdate && (
                <span className="px-1.5 py-[1px] rounded-md bg-warning-bg text-warning text-[10px] font-medium">
                  当前 v{installedVersion} · 可更新
                </span>
              )}
            </div>
            <span className="text-[11px] text-muted-foreground">by {item.author} · {item.category}</span>
          </div>
          {item.appType === 'automation' ? (
            <button
              type="button"
              onClick={openInstallWizard}
              className={cn(
                'flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium',
                'bg-primary text-primary-foreground hover:bg-primary/90 transition-colors',
              )}
            >
              <Download size={12} />
              {isInstalled && !hasUpdate ? '重新安装' : hasUpdate ? '更新到 v' + item.version : '安装'}
            </button>
          ) : (
            <span className="text-[11px] text-muted-foreground italic">
              {item.appType.toUpperCase()} 安装在 Phase 3b 开放
            </span>
          )}
        </div>
        {/* Sub-tab strip */}
        <div className="flex items-center gap-1 px-6 pb-2">
          {TABS.map((tab) => {
            const active = activeTab === tab.id
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  'relative px-3 py-1 text-[12px] rounded-md transition-colors',
                  active
                    ? 'bg-muted text-foreground font-medium'
                    : 'text-muted-foreground hover:text-foreground hover:bg-accent/30',
                )}
              >
                {active && <span className="absolute left-0 top-1.5 bottom-1.5 w-[2px] bg-primary rounded-r" />}
                {tab.label}
              </button>
            )
          })}
        </div>
      </div>

      {/* Sub-tab content (fade transitions) */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        <AnimatePresence mode="wait">
          <motion.div
            key={activeTab}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18, ease: [0.32, 0.72, 0, 1] }}
            className="max-w-3xl"
          >
            {activeTab === 'overview' && (
              <div className="space-y-4">
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">描述</h3>
                  <p className="text-[13px] text-foreground/90 leading-relaxed">{displayDesc}</p>
                </section>
                {item.tags.length > 0 && (
                  <section>
                    <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">标签</h3>
                    <div className="flex flex-wrap gap-1.5">
                      {item.tags.map((tag) => (
                        <span key={tag} className="text-[11px] px-2 py-0.5 rounded-full bg-secondary text-muted-foreground">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </section>
                )}
                <section className="grid grid-cols-2 gap-x-6 gap-y-2 text-[12px]">
                  <Row label="作者" value={item.author} />
                  <Row label="版本" value={`v${item.version}`} />
                  <Row label="分类" value={item.category} />
                  <Row label="语言" value={item.locale ?? '未指定'} />
                  {item.minAppVersion && <Row label="最低 uClaw 版本" value={item.minAppVersion} />}
                </section>
              </div>
            )}

            {activeTab === 'config' && (
              <div>
                <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">配置项预览</h3>
                <ConfigSchemaPreview parsedSpecJson={parsedSpecJson} specI18n={specI18n} locale={locale} />
              </div>
            )}

            {activeTab === 'requires' && (
              <div className="space-y-4">
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
                    MCP 服务 ({requiresMcps.length})
                  </h3>
                  {requiresMcps.length === 0 ? (
                    <p className="text-[12px] text-muted-foreground italic">无</p>
                  ) : (
                    <ul className="space-y-1 text-[12px]">
                      {requiresMcps.map((m) => (
                        <li key={m} className="px-3 py-2 rounded-md bg-card border border-border/50">
                          {m}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
                <section>
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider mb-2">
                    依赖技能 ({requiresSkills.length})
                  </h3>
                  {requiresSkills.length === 0 ? (
                    <p className="text-[12px] text-muted-foreground italic">无</p>
                  ) : (
                    <ul className="space-y-1 text-[12px]">
                      {requiresSkills.map((s) => (
                        <li key={s} className="px-3 py-2 rounded-md bg-card border border-border/50">
                          {s}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              </div>
            )}

            {activeTab === 'prompt' && (
              <div>
                <div className="flex items-center justify-between mb-2">
                  <h3 className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">系统提示词</h3>
                  <button
                    type="button"
                    onClick={() => setPromptExpanded((v) => !v)}
                    className="text-[11px] text-primary hover:underline"
                  >
                    {promptExpanded ? '折叠' : '展开'}
                  </button>
                </div>
                <pre className={cn(
                  'text-[11px] font-mono text-foreground/80 whitespace-pre-wrap',
                  'px-3 py-2 rounded-md bg-card border border-border/50',
                  !promptExpanded && 'max-h-[200px] overflow-hidden',
                )}>
                  {parsedSpecJson && typeof parsedSpecJson === 'object' && parsedSpecJson !== null && 'system_prompt' in parsedSpecJson
                    ? String((parsedSpecJson as Record<string, unknown>).system_prompt)
                    : specYaml.slice(0, 2000) + (specYaml.length > 2000 ? '\n...' : '')}
                </pre>
              </div>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
      <InstallWizard />
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }): React.ReactElement {
  return (
    <div className="flex items-center justify-between gap-3 py-1 border-b border-border/30">
      <span className="text-muted-foreground shrink-0">{label}</span>
      <span className="text-foreground/80 truncate text-right">{value}</span>
    </div>
  )
}

function ConfigSchemaPreview({
  parsedSpecJson,
  specI18n,
  locale,
}: {
  parsedSpecJson: unknown | null
  specI18n: SpecI18n | undefined
  locale: string
}): React.ReactElement {
  if (!parsedSpecJson) {
    return (
      <div className="flex items-start gap-2 p-3 rounded-md bg-warning-bg text-warning text-[12px]">
        <AlertTriangle size={14} className="mt-0.5 shrink-0" />
        <span>spec.yaml 解析失败，配置预览不可用。安装时会回退到默认配置。</span>
      </div>
    )
  }
  const obj = parsedSpecJson as Record<string, unknown>
  const schema = obj.config_schema
  if (!Array.isArray(schema) || schema.length === 0) {
    return <p className="text-[12px] text-muted-foreground italic">此数字员工无可配置项</p>
  }
  return (
    <ul className="space-y-2">
      {(schema as ConfigSchemaEntry[]).map((entry, idx) => {
        const label = localizeConfig(entry.key, 'label', entry.label, specI18n, locale)
        const description = localizeConfig(entry.key, 'description', entry.description, specI18n, locale)
        return (
          <li key={idx} className="px-3 py-2 rounded-md bg-card border border-border/50 text-[12px]">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-medium">{label || entry.key}</span>
              <span className="text-[10px] text-muted-foreground px-1.5 py-[1px] rounded bg-muted">
                {String(entry.type ?? 'unknown')}
              </span>
              {entry.required ? (
                <span className="text-[10px] text-danger">必填</span>
              ) : (
                <span className="text-[10px] text-muted-foreground/70">可选</span>
              )}
            </div>
            {description && (
              <p className="text-[11px] text-muted-foreground">{description}</p>
            )}
            {entry.options && entry.options.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-1">
                {entry.options.map((opt) => (
                  <span key={opt.value} className="text-[10px] px-1.5 py-[1px] rounded bg-muted text-muted-foreground">
                    {localizeOption(entry.key, opt.value, opt.label, specI18n, locale)}
                  </span>
                ))}
              </div>
            )}
            {entry.default != null && (
              <p className="text-[11px] text-muted-foreground mt-1">
                默认: <span className="font-mono">{String(entry.default)}</span>
              </p>
            )}
          </li>
        )
      })}
    </ul>
  )
}

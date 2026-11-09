/**
 * SkillsMarketplaceTab — 万花筒「技能」模块的「市场」分页。
 *
 * - provider 切换：skills.sh（默认）/ skillsmp.com。
 * - skills.sh 空关键词 → 热门榜（需要 API Key；没有 Key 时提示用关键词搜索）；
 *   skillsmp 没有榜单，空关键词只显示提示。非空关键词 300ms 防抖后搜索。
 * - 每行 [全局] / [本工作区] 安装；点行体打开详情抽屉。
 * - 「已安装」列出市场装过的技能（V25 表 itemType === 'skill'），卸载前内联确认。
 * - 后端错误本身就是给用户看的中文，原样显示。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { KeyRound, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import {
  installSkillFromMarketplace,
  listSkillsMarketplace,
  listStandaloneInstalls,
  searchSkillsMarketplace,
  uninstallSkillFromMarketplace,
  type StandaloneInstall,
} from '@/lib/tauri-bridge'
import type {
  MarketplaceInstallScope,
  MarketplaceProvider,
  MarketplaceSkillSummary,
} from '@/lib/types'
import { SkillMarketplaceDetailDrawer } from './SkillMarketplaceDetailDrawer'
import { MarketplaceApiKeysDialog } from './MarketplaceApiKeysDialog'

type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: MarketplaceInstallScope }
  | { kind: 'installed'; scope: MarketplaceInstallScope }
  | { kind: 'error'; message: string }

type UninstallState =
  | { kind: 'idle' }
  | { kind: 'confirming' }
  | { kind: 'uninstalling' }
  | { kind: 'error'; message: string }

const PROVIDER_TABS: { value: MarketplaceProvider; label: string }[] = [
  { value: 'skills_sh', label: 'skills.sh' },
  { value: 'skillsmp', label: 'skillsmp.com' },
]

const SCOPE_LABEL: Record<MarketplaceInstallScope, string> = { global: '全局', workspace: '本工作区' }

const pill =
  'text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50'

export function SkillsMarketplaceTab({
  query,
  onError,
  refreshSignal = 0,
}: {
  query: string
  onError?: (m: string) => void
  /** Bump to refetch the 已安装 list (e.g. after an in-chat install). */
  refreshSignal?: number
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [provider, setProvider] = React.useState<MarketplaceProvider>('skills_sh')
  const [rows, setRows] = React.useState<MarketplaceSkillSummary[]>([])
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)
  const [hint, setHint] = React.useState<string | null>(null)
  const [states, setStates] = React.useState<Record<string, RowState>>({})
  const [detail, setDetail] = React.useState<MarketplaceSkillSummary | null>(null)
  const [installed, setInstalled] = React.useState<StandaloneInstall[]>([])
  const [uninstallStates, setUninstallStates] = React.useState<Record<string, UninstallState>>({})
  const [refreshKey, setRefreshKey] = React.useState(0)
  const [keysOpen, setKeysOpen] = React.useState(false)
  // Bumped after a key is saved so the current list/search reruns with it.
  const [reloadKey, setReloadKey] = React.useState(0)

  React.useEffect(() => {
    let cancelled = false
    const q = query.trim()
    setStates({})

    if (!q && provider === 'skillsmp') {
      setLoading(false)
      setError(null)
      setRows([])
      setHint('输入关键词搜索 skillsmp.com')
      return () => {
        cancelled = true
      }
    }

    const run = async () => {
      setLoading(true)
      setError(null)
      setHint(null)
      try {
        const result = q
          ? await searchSkillsMarketplace(q, 20, provider)
          : await listSkillsMarketplace('trending', 0, provider)
        if (!cancelled) setRows(result)
      } catch (e) {
        if (cancelled) return
        const message = String(e)
        setRows([])
        // Trending needs a skills.sh key; without one, steer to search instead.
        if (!q && /API Key/.test(message)) {
          setHint('输入关键词搜索 skills.sh；填写 API Key 后可浏览热门技能')
        } else {
          setError(message)
          onError?.(message)
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    if (!q) {
      void run()
      return () => {
        cancelled = true
      }
    }
    const timer = setTimeout(() => void run(), 300)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
    // onError is a fresh closure each render; refetch only on query/provider.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, provider, reloadKey])

  React.useEffect(() => {
    let cancelled = false
    listStandaloneInstalls()
      .then((all) => {
        if (!cancelled) setInstalled(all.filter((it) => it.itemType === 'skill'))
      })
      .catch(() => {
        if (!cancelled) setInstalled([])
      })
    return () => {
      cancelled = true
    }
  }, [refreshKey, refreshSignal])

  const install = async (rowKey: string, row: MarketplaceSkillSummary, scope: MarketplaceInstallScope) => {
    setStates((s) => ({ ...s, [rowKey]: { kind: 'installing', scope } }))
    try {
      await installSkillFromMarketplace(
        row.id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
        provider,
        row.installUrl,
      )
      setStates((s) => ({ ...s, [rowKey]: { kind: 'installed', scope } }))
      setRefreshKey((k) => k + 1)
    } catch (e) {
      setStates((s) => ({ ...s, [rowKey]: { kind: 'error', message: String(e) } }))
    }
  }

  const uninstall = async (slug: string) => {
    setUninstallStates((s) => ({ ...s, [slug]: { kind: 'uninstalling' } }))
    try {
      await uninstallSkillFromMarketplace(slug)
      setUninstallStates((s) => ({ ...s, [slug]: { kind: 'idle' } }))
      setRefreshKey((k) => k + 1)
    } catch (e) {
      setUninstallStates((s) => ({ ...s, [slug]: { kind: 'error', message: String(e) } }))
    }
  }

  return (
    <div className="titlebar-no-drag flex-1 min-h-0 overflow-y-auto px-5 md:px-8 py-4">
      {installed.length > 0 && (
        <div className="mb-4">
          <div className="text-xs font-medium text-muted-foreground mb-1.5 px-1">已安装</div>
          <div className="flex flex-col gap-1.5">
            {installed.map((it) => {
              const ust: UninstallState = uninstallStates[it.slug] ?? { kind: 'idle' }
              return (
                <div
                  key={it.slug}
                  className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3"
                >
                  <div className="min-w-0">
                    <div className="text-sm text-foreground truncate">{it.slug}</div>
                    {ust.kind === 'error' && (
                      <div className="text-xs text-red-400/90 mt-0.5 break-words">{ust.message}</div>
                    )}
                  </div>
                  {ust.kind === 'confirming' ? (
                    <div className="shrink-0 flex items-center gap-1.5">
                      <span className="text-xs text-muted-foreground">确认卸载？</span>
                      <button
                        type="button"
                        onClick={() => void uninstall(it.slug)}
                        className="text-xs px-2.5 py-1 rounded-lg bg-red-500/15 text-red-500 hover:bg-red-500/25 transition-colors"
                      >
                        确认
                      </button>
                      <button
                        type="button"
                        onClick={() => setUninstallStates((s) => ({ ...s, [it.slug]: { kind: 'idle' } }))}
                        className={pill}
                      >
                        取消
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() => setUninstallStates((s) => ({ ...s, [it.slug]: { kind: 'confirming' } }))}
                      disabled={ust.kind === 'uninstalling'}
                      className={cn('shrink-0', pill)}
                    >
                      {ust.kind === 'uninstalling' ? '…' : '卸载'}
                    </button>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      )}

      <div className="flex items-center gap-1 mb-3">
        {PROVIDER_TABS.map((tab) => (
          <button
            key={tab.value}
            type="button"
            onClick={() => setProvider(tab.value)}
            className={cn(
              'rounded-full px-2.5 py-1 text-[10.5px] font-medium transition-all duration-200 whitespace-nowrap shrink-0 active:scale-95',
              provider === tab.value
                ? 'bg-primary text-primary-foreground shadow-[0_1px_4px_rgba(0,0,0,0.1)]'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/60',
            )}
          >
            {tab.label}
          </button>
        ))}
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => setKeysOpen(true)}
          className="inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-[10.5px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
        >
          <KeyRound className="size-3" />
          API Key
        </button>
      </div>

      {loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground px-1 py-2">
          <Loader2 className="size-3.5 animate-spin shrink-0" />
          加载中…
        </div>
      ) : error ? (
        <div className="text-xs text-red-500/90 bg-red-500/10 rounded-lg px-3 py-2">{error}</div>
      ) : hint ? (
        <div className="text-xs text-muted-foreground px-1 py-1">{hint}</div>
      ) : rows.length === 0 ? (
        <div className="text-xs text-muted-foreground px-1 py-1">没有匹配的技能</div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {rows.map((row, i) => {
            const rowKey = `${row.id}__${i}`
            const st: RowState = states[rowKey] ?? { kind: 'idle' }
            const busy = st.kind === 'installing'
            return (
              <div
                key={rowKey}
                onClick={() => setDetail(row)}
                className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3 cursor-pointer hover:bg-muted/30 transition-colors"
              >
                <div className="min-w-0">
                  <div className="text-sm text-foreground truncate">{row.name}</div>
                  <div className="text-xs text-muted-foreground truncate">
                    {row.source}
                    {row.installs > 0 && ` · ${row.installs.toLocaleString()} ${provider === 'skillsmp' ? '星' : '次安装'}`}
                  </div>
                  {row.description && (
                    <div className="text-xs text-muted-foreground/80 truncate mt-0.5">{row.description}</div>
                  )}
                  {st.kind === 'error' && (
                    <div className="text-xs text-red-500/90 mt-0.5 break-words">{st.message}</div>
                  )}
                </div>
                {st.kind === 'installed' ? (
                  <span className="shrink-0 text-xs px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                    已安装（{SCOPE_LABEL[st.scope]}）
                  </span>
                ) : (
                  <div className="shrink-0 flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
                    <button type="button" onClick={() => void install(rowKey, row, 'global')} disabled={busy} className={pill}>
                      {busy && st.scope === 'global' ? '…' : '全局'}
                    </button>
                    <button
                      type="button"
                      onClick={() => void install(rowKey, row, 'workspace')}
                      disabled={busy || !activeWorkspaceId}
                      title={activeWorkspaceId ? '安装到当前工作区' : '没有选中的工作区'}
                      className={pill}
                    >
                      {busy && st.scope === 'workspace' ? '…' : '本工作区'}
                    </button>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      <SkillMarketplaceDetailDrawer
        open={detail !== null}
        onOpenChange={(open) => {
          if (!open) setDetail(null)
        }}
        skill={detail}
        provider={provider}
        onInstalled={() => setRefreshKey((k) => k + 1)}
      />
      <MarketplaceApiKeysDialog
        open={keysOpen}
        onOpenChange={setKeysOpen}
        onSaved={() => setReloadKey((k) => k + 1)}
      />
    </div>
  )
}

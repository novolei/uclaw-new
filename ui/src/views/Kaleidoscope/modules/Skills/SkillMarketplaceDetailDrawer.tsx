/**
 * SkillMarketplaceDetailDrawer — 市场技能详情：SKILL.md 预览、审计风险徽章、
 * [全局]/[本工作区] 安装（高风险先内联确认）。详情与审计并行拉取，各自失败互不影响。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import {
  getSkillMarketplaceAudit,
  getSkillMarketplaceDetail,
  installSkillFromMarketplace,
} from '@/lib/tauri-bridge'
import type {
  MarketplaceInstallScope,
  MarketplaceProvider,
  MarketplaceSkillAudit,
  MarketplaceSkillDetail,
  MarketplaceSkillSummary,
} from '@/lib/types'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { cn } from '@/lib/utils'

type InstallState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: MarketplaceInstallScope }
  | { kind: 'installed'; scope: MarketplaceInstallScope }
  | { kind: 'error'; message: string }

type Risk = 'HIGH' | 'MEDIUM' | 'LOW' | 'NONE'

/** Highest risk among audit entries; NONE when unaudited. */
export function highestRisk(audit: MarketplaceSkillAudit | null): Risk {
  const levels = (audit?.audits ?? []).map((a) => a.riskLevel.toUpperCase())
  if (levels.includes('HIGH')) return 'HIGH'
  if (levels.includes('MEDIUM')) return 'MEDIUM'
  if (levels.includes('LOW')) return 'LOW'
  return 'NONE'
}

const RISK_BADGE: Record<Risk, { label: string; className: string }> = {
  HIGH: { label: '⚠ 高风险', className: 'bg-red-500/15 text-red-600 border-red-500/30 dark:text-red-400' },
  MEDIUM: { label: '⚠ 中风险', className: 'bg-amber-500/15 text-amber-700 border-amber-500/30 dark:text-amber-400' },
  LOW: { label: '✓ 低风险', className: 'bg-emerald-500/15 text-emerald-700 border-emerald-500/30 dark:text-emerald-400' },
  NONE: { label: '未审计', className: 'bg-muted text-muted-foreground border-border' },
}

const pill =
  'text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50'

export function SkillMarketplaceDetailDrawer({
  open,
  onOpenChange,
  skill,
  provider,
  onInstalled,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  skill: MarketplaceSkillSummary | null
  provider: MarketplaceProvider
  onInstalled?: () => void
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [detail, setDetail] = React.useState<MarketplaceSkillDetail | null>(null)
  const [audit, setAudit] = React.useState<MarketplaceSkillAudit | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [detailErr, setDetailErr] = React.useState<string | null>(null)
  const [installState, setInstallState] = React.useState<InstallState>({ kind: 'idle' })
  const [pendingScope, setPendingScope] = React.useState<MarketplaceInstallScope | null>(null)

  React.useEffect(() => {
    if (!open || !skill) return
    let cancelled = false
    setLoading(true)
    setDetail(null)
    setAudit(null)
    setDetailErr(null)
    setInstallState({ kind: 'idle' })
    setPendingScope(null)
    void Promise.allSettled([
      getSkillMarketplaceDetail(skill.id, provider, skill.installUrl),
      getSkillMarketplaceAudit(skill.id, provider),
    ]).then(([d, a]) => {
      if (cancelled) return
      if (d.status === 'fulfilled') setDetail(d.value)
      else setDetailErr(String(d.reason))
      if (a.status === 'fulfilled') setAudit(a.value)
      setLoading(false)
    })
    return () => {
      cancelled = true
    }
  }, [open, skill, provider])

  const risk = highestRisk(audit)
  const badge = RISK_BADGE[risk]
  const skillMd = detail?.files.find((f) => f.path === 'SKILL.md') ?? detail?.files[0] ?? null

  const doInstall = async (scope: MarketplaceInstallScope) => {
    if (!skill) return
    setInstallState({ kind: 'installing', scope })
    try {
      await installSkillFromMarketplace(
        skill.id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
        provider,
        skill.installUrl,
      )
      setInstallState({ kind: 'installed', scope })
      onInstalled?.()
    } catch (e) {
      setInstallState({ kind: 'error', message: String(e) })
    }
  }

  const requestInstall = (scope: MarketplaceInstallScope) => {
    if (risk === 'HIGH') setPendingScope(scope)
    else void doInstall(scope)
  }

  const busy = installState.kind === 'installing'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full sm:max-w-md flex flex-col gap-0 p-0">
        <SheetHeader className="px-6 pt-6 pb-4 border-b border-border text-left">
          <SheetTitle className="truncate pr-8">{skill?.name ?? '技能详情'}</SheetTitle>
          <div className="flex items-center gap-2 flex-wrap">
            {skill?.source && <span className="text-xs text-muted-foreground truncate">{skill.source}</span>}
            <span className={cn('shrink-0 text-[10.5px] px-2 py-0.5 rounded-full border font-medium', badge.className)}>
              {badge.label}
            </span>
          </div>
        </SheetHeader>

        <div className="flex-1 min-h-0 overflow-y-auto px-6 py-4">
          {loading ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground py-2">
              <Loader2 className="size-3.5 animate-spin shrink-0" />
              加载中…
            </div>
          ) : detailErr ? (
            <div className="text-xs text-red-500/90 bg-red-500/10 rounded-lg px-3 py-2">{detailErr}</div>
          ) : skillMd ? (
            <>
              <div className="mb-2 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                {skillMd.path}
              </div>
              <pre className="text-[11.5px] leading-relaxed text-foreground/85 whitespace-pre-wrap break-words bg-muted/30 rounded-lg p-3 font-mono">
                {skillMd.contents.slice(0, 20000)}
              </pre>
            </>
          ) : (
            <div className="text-xs text-muted-foreground py-1">无预览内容</div>
          )}
        </div>

        <div className="shrink-0 border-t border-border px-6 py-4">
          {pendingScope ? (
            <div className="flex flex-col gap-2">
              <div className="text-xs text-red-500/90">此技能审计为高风险，仍要安装？</div>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => {
                    const scope = pendingScope
                    setPendingScope(null)
                    void doInstall(scope)
                  }}
                  className="text-xs px-2.5 py-1 rounded-lg bg-red-500/15 text-red-600 dark:text-red-400 hover:bg-red-500/25 transition-colors"
                >
                  确认安装
                </button>
                <button type="button" onClick={() => setPendingScope(null)} className={pill}>
                  取消
                </button>
              </div>
            </div>
          ) : installState.kind === 'installed' ? (
            <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
              已安装（{installState.scope === 'global' ? '全局' : '本工作区'}）
            </span>
          ) : (
            <div className="flex items-center gap-1.5 flex-wrap">
              <button type="button" onClick={() => requestInstall('global')} disabled={busy || !skill} className={pill}>
                {busy && installState.scope === 'global' ? '…' : '安装到全局'}
              </button>
              <button
                type="button"
                onClick={() => requestInstall('workspace')}
                disabled={busy || !skill || !activeWorkspaceId}
                title={activeWorkspaceId ? '安装到当前工作区' : '没有选中的工作区'}
                className={pill}
              >
                {busy && installState.scope === 'workspace' ? '…' : '安装到本工作区'}
              </button>
              {installState.kind === 'error' && (
                <span className="text-xs text-red-500/90 break-words">{installState.message}</span>
              )}
            </div>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}

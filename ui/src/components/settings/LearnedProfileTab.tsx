/**
 * LearnedProfileTab — surfaces the openhuman-style FacetCache produced by
 * Memory OS Sprint 1's stability_detector pipeline.
 *
 * Layout:
 *   - Header — explanatory subtitle, total active count, "Rebuild now"
 *     button + "Refresh" button.
 *   - Class-grouped list (Identity / Style / Tooling / Veto / Goal /
 *     Channel) with one row per facet showing
 *     `{name}: {value}` + state badge + stability score + evidence count
 *     + per-row "Dismiss" button.
 *   - Empty state explaining the cache fills as the user chats.
 *
 * The rebuild button triggers `memory_learning_rebuild_now` (a 30-min
 * cadence runs automatically via ProactiveService, so this is a "I
 * want to see new facets right now" affordance). Failing to rebuild
 * with `learning_enabled=false` returns a structured error which we
 * surface inline rather than crashing.
 *
 * Sprint 2.2. Closes the visibility gap left by Sprint 1.10 (which
 * shipped the IPC but no UI), making the producer→consumer pipeline
 * end-to-end visible to the user.
 */
import * as React from 'react'
import { toast } from 'sonner'
import {
  Loader2,
  RefreshCw,
  UserCircle2,
  X,
  ChevronUp,
  ChevronDown,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { cn, formatDateTime } from '@/lib/utils'
import {
  memoryLearningListFacets,
  memoryLearningDismissFacet,
  memoryLearningRebuildNow,
  memoryLearningPromoteFacet,
  memoryLearningDemoteFacet,
} from '@/lib/tauri-bridge'
import type { FacetDto } from '@/lib/types'

// ─── Class taxonomy ────────────────────────────────────────────────────

/** Render order matches the Rust `CLASS_RENDER_ORDER` in
 *  `learning::prompt_section`. Stable ordering means the user always
 *  sees the same sections in the same place even when an earlier
 *  class is empty (it shows "(none yet)" instead of disappearing). */
const CLASS_RENDER_ORDER: ReadonlyArray<string> = [
  'identity',
  'style',
  'tooling',
  'veto',
  'goal',
  'channel',
]

const CLASS_LABEL: Record<string, string> = {
  identity: '身份 (Identity)',
  style: '风格 (Style)',
  tooling: '工具 (Tooling)',
  veto: '禁忌 (Veto)',
  goal: '目标 (Goal)',
  channel: '渠道 (Channel)',
}

const CLASS_DESCRIPTION: Record<string, string> = {
  identity: '你是谁 — 名字、职位、角色',
  style: '语言、长度、语气偏好',
  tooling: '常用工具、库、编辑器',
  veto: '不要做的事、不要用的工具',
  goal: '当前在做的事 / 关心的项目',
  channel: '消息渠道偏好（IM、邮件等）',
}

// ─── State badge ───────────────────────────────────────────────────────

function stateBadgeTone(state: string): string {
  switch (state.toLowerCase()) {
    case 'active':
      return 'bg-green-500/15 text-green-700 dark:text-green-300 border-green-500/30'
    case 'provisional':
      return 'bg-amber-500/15 text-amber-700 dark:text-amber-300 border-amber-500/30'
    case 'candidate':
      return 'bg-muted/40 text-muted-foreground border-border/50'
    case 'forgotten':
      return 'bg-muted/20 text-muted-foreground/60 border-border/30 line-through'
    default:
      return 'bg-muted/40 text-muted-foreground border-border/50'
  }
}

// ─── Component ─────────────────────────────────────────────────────────

export function LearnedProfileTab(): React.ReactElement {
  const [facets, setFacets] = React.useState<FacetDto[]>([])
  const [loading, setLoading] = React.useState<boolean>(true)
  const [rebuilding, setRebuilding] = React.useState<boolean>(false)
  const [error, setError] = React.useState<string | null>(null)
  const [dismissing, setDismissing] = React.useState<Set<string>>(new Set())

  const fetchFacets = React.useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const list = await memoryLearningListFacets({})
      setFacets(Array.isArray(list) ? list : [])
    } catch (e) {
      setError(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void fetchFacets()
  }, [fetchFacets])

  const handleRebuild = async (): Promise<void> => {
    setRebuilding(true)
    setError(null)
    try {
      await memoryLearningRebuildNow({})
      await fetchFacets()
      toast.success('已重建 — Profile 已根据最新候选刷新')
    } catch (e) {
      const msg = String(e)
      setError(msg)
      // The structured "learning disabled" error is friendlier as a toast.
      if (msg.toLowerCase().includes('disabled')) {
        toast.error('学习管线已关闭 — 在「智能」页打开 memory_os.learning_enabled')
      } else {
        toast.error(`重建失败: ${msg}`)
      }
    } finally {
      setRebuilding(false)
    }
  }

  const handleDismiss = async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningDismissFacet({ facetId })
      // Optimistic: flip state to "forgotten" locally so the row dims
      // but stays — matches the backend (it doesn't delete, just flags).
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'forgotten' } : f,
        ),
      )
    } catch (e) {
      toast.error(`移除失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }

  // Sprint 2.3 — promote / demote share the same shape as dismiss. The
  // optimistic local update mirrors the backend's UPDATE: state column
  // flips, the row stays. Next stability rebuild can override.
  const handlePromote = async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningPromoteFacet({ facetId })
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'active' } : f,
        ),
      )
    } catch (e) {
      toast.error(`提升失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }

  const handleDemote = async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningDemoteFacet({ facetId })
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'provisional' } : f,
        ),
      )
    } catch (e) {
      toast.error(`降级失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }

  // ─── Group facets by class ───────────────────────────────────────
  const grouped = React.useMemo(() => {
    const buckets = new Map<string, FacetDto[]>()
    for (const f of facets) {
      const key = f.class.toLowerCase()
      const arr = buckets.get(key) ?? []
      arr.push(f)
      buckets.set(key, arr)
    }
    // Sort each bucket: active first, then provisional, then by
    // stability descending so the strongest evidence sits on top.
    for (const [k, arr] of buckets) {
      arr.sort((a, b) => {
        const stateOrder: Record<string, number> = {
          active: 0,
          provisional: 1,
          candidate: 2,
          forgotten: 3,
        }
        const sa = stateOrder[a.state.toLowerCase()] ?? 99
        const sb = stateOrder[b.state.toLowerCase()] ?? 99
        if (sa !== sb) return sa - sb
        return b.stability - a.stability
      })
      buckets.set(k, arr)
    }
    return buckets
  }, [facets])

  const activeCount = facets.filter(
    (f) => f.state.toLowerCase() === 'active',
  ).length
  const provisionalCount = facets.filter(
    (f) => f.state.toLowerCase() === 'provisional',
  ).length

  return (
    <div className="space-y-6" data-testid="learned-profile-tab">
      {/* Header */}
      <section data-settings-section="学到的偏好">
        <div className="flex items-start justify-between gap-3 mb-3">
          <div className="flex items-start gap-2">
            <UserCircle2 className="size-5 text-muted-foreground mt-0.5" />
            <div>
              <h2 className="text-sm font-medium text-foreground">学到的偏好</h2>
              <p className="text-xs text-muted-foreground mt-1 max-w-prose">
                我从对话中学到的关于你的偏好。每 30 分钟根据稳定性自动重建，
                也会写到 <code className="text-[11px] bg-muted/40 px-1 py-0.5 rounded">~/Documents/workground/brain/PROFILE.md</code>。
                不想要的可以「移除」 — 下次出现足够新证据时还会再次浮现。
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <Button
              size="sm"
              variant="ghost"
              className="text-xs h-7 gap-1"
              onClick={() => void fetchFacets()}
              disabled={loading}
              title="刷新当前缓存（不重建）"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
              刷新
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="text-xs h-7 gap-1"
              onClick={() => void handleRebuild()}
              disabled={rebuilding}
              title="手动触发稳定性重建（默认 30 分钟一次）"
            >
              <RefreshCw className={cn('size-3', rebuilding && 'animate-spin')} />
              立即重建
            </Button>
          </div>
        </div>

        {/* Summary badges */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Badge variant="outline" className="text-[10px] px-1.5 py-0">
            {activeCount} active
          </Badge>
          {provisionalCount > 0 && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0">
              {provisionalCount} provisional
            </Badge>
          )}
          <span className="text-[10px] text-muted-foreground/70">
            共 {facets.length} 条
          </span>
        </div>

        {/* Error banner */}
        {error && (
          <div className="mt-3 px-3 py-2 bg-destructive/10 text-destructive text-xs rounded">
            {error}
          </div>
        )}
      </section>

      {/* Empty state */}
      {!loading && facets.length === 0 && !error && <EmptyState />}

      {/* Loading state */}
      {loading && facets.length === 0 && (
        <div className="flex items-center justify-center py-10">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      )}

      {/* Class groups */}
      {!loading && facets.length > 0 && (
        <div className="space-y-5">
          {CLASS_RENDER_ORDER.map((cls) => {
            const items = grouped.get(cls) ?? []
            return (
              <ClassGroup
                key={cls}
                className={cls}
                facets={items}
                dismissing={dismissing}
                onDismiss={handleDismiss}
                onPromote={handlePromote}
                onDemote={handleDemote}
              />
            )
          })}
          {/* Forward-compat: any unknown class (future backend changes) */}
          {Array.from(grouped.entries())
            .filter(([k]) => !CLASS_RENDER_ORDER.includes(k))
            .map(([k, items]) => (
              <ClassGroup
                key={k}
                className={k}
                facets={items}
                dismissing={dismissing}
                onDismiss={handleDismiss}
                onPromote={handlePromote}
                onDemote={handleDemote}
              />
            ))}
        </div>
      )}
    </div>
  )
}

// ─── EmptyState ────────────────────────────────────────────────────────

function EmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center gap-2 py-10 text-center px-4 border border-dashed border-border/50 rounded-md">
      <UserCircle2 className="size-8 text-muted-foreground/40" />
      <p className="text-xs text-muted-foreground">还没有学到任何偏好。</p>
      <p className="text-[10px] text-muted-foreground/60 max-w-prose">
        随着你和 uClaw 对话，提取器会从消息中提取候选事实（如 "我用 helix"、"我叫 Alice"），
        每 30 分钟根据证据稳定性把候选晋级为 active。
      </p>
    </div>
  )
}

// ─── ClassGroup ────────────────────────────────────────────────────────

interface ClassGroupProps {
  className: string
  facets: FacetDto[]
  dismissing: Set<string>
  onDismiss: (id: string) => void
  onPromote: (id: string) => void
  onDemote: (id: string) => void
}

function ClassGroup({
  className,
  facets,
  dismissing,
  onDismiss,
  onPromote,
  onDemote,
}: ClassGroupProps): React.ReactElement {
  const label = CLASS_LABEL[className] ?? className
  const description = CLASS_DESCRIPTION[className]
  return (
    <section data-class-group={className}>
      <div className="mb-2">
        <h3 className="text-xs font-medium text-foreground">{label}</h3>
        {description && (
          <p className="text-[10px] text-muted-foreground/70 mt-0.5">
            {description}
          </p>
        )}
      </div>
      {facets.length === 0 ? (
        <p className="text-[11px] text-muted-foreground/50 italic px-2 py-1">
          （还没学到）
        </p>
      ) : (
        <ul className="space-y-1">
          {facets.map((f) => (
            <FacetRow
              key={f.facetId}
              facet={f}
              busy={dismissing.has(f.facetId)}
              onDismiss={() => onDismiss(f.facetId)}
              onPromote={() => onPromote(f.facetId)}
              onDemote={() => onDemote(f.facetId)}
            />
          ))}
        </ul>
      )}
    </section>
  )
}

// ─── FacetRow ──────────────────────────────────────────────────────────

interface FacetRowProps {
  facet: FacetDto
  busy: boolean
  onDismiss: () => void
  onPromote: () => void
  onDemote: () => void
}

function FacetRow({
  facet,
  busy,
  onDismiss,
  onPromote,
  onDemote,
}: FacetRowProps): React.ReactElement {
  const state = facet.state.toLowerCase()
  const forgotten = state === 'forgotten'
  // Sprint 2.3 — visibility rules: promote when state is anything other
  // than 'active' (i.e. provisional / candidate / forgotten — all can be
  // lifted), demote only from 'active' / 'provisional' (no-op below).
  const canPromote = state !== 'active'
  const canDemote = state === 'active' || state === 'provisional'
  return (
    <li
      className={cn(
        'group flex items-center justify-between gap-3 px-3 py-2 rounded-md bg-muted/20 border border-border/30',
        forgotten && 'opacity-60',
      )}
      data-facet-id={facet.facetId}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs font-medium text-foreground truncate">
            {facet.name}
          </span>
          <span className="text-xs text-muted-foreground">:</span>
          <span
            className={cn(
              'text-xs text-foreground truncate',
              forgotten && 'line-through',
            )}
          >
            {facet.value}
          </span>
        </div>
        <div className="flex items-center gap-2 mt-1 text-[10px] text-muted-foreground/70">
          <span
            className={cn(
              'px-1.5 py-0 border rounded text-[10px]',
              stateBadgeTone(facet.state),
            )}
          >
            {facet.state}
          </span>
          <span>stability {facet.stability.toFixed(2)}</span>
          <span>· evidence {facet.evidenceCount}</span>
          <span>· {formatDateTime(facet.lastSeenAtMs)}</span>
        </div>
      </div>
      {/* Sprint 2.3 — action cluster: promote / demote / dismiss.
          Buttons reveal on row-hover (group-hover:opacity-100) so the
          row stays visually quiet at rest. The currently-busy button
          shows its spinner regardless of hover. */}
      <div
        className={cn(
          'flex items-center gap-0.5 transition-opacity',
          busy ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
        )}
      >
        {canPromote && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-green-600 dark:hover:text-green-400"
            onClick={onPromote}
            disabled={busy}
            title="提升为 active — 下次系统提示词会包含它"
            aria-label={`promote-${facet.facetId}`}
          >
            <ChevronUp className="size-3.5" />
          </Button>
        )}
        {canDemote && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-amber-600 dark:hover:text-amber-400"
            onClick={onDemote}
            disabled={busy}
            title="降级为 provisional — 不再出现在系统提示词里，但保留观察"
            aria-label={`demote-${facet.facetId}`}
          >
            <ChevronDown className="size-3.5" />
          </Button>
        )}
        {!forgotten && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
            onClick={onDismiss}
            disabled={busy}
            title="标记为「忘掉」（下次有新证据时还会再出现）"
            aria-label={`dismiss-${facet.facetId}`}
          >
            {busy ? <Loader2 className="size-3 animate-spin" /> : <X className="size-3" />}
          </Button>
        )}
      </div>
    </li>
  )
}

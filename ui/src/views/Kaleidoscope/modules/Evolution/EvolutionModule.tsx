/**
 * EvolutionModule — 万花筒「进化」模块。
 *
 * 展示 GEP Gene 进化引擎的状态：左侧 Gene 列表 + 右侧详情 / Capsules / 版本树。
 */
import * as React from 'react'
import { toast } from 'sonner'
import { Search, Trash2, RotateCcw, Loader2 } from 'lucide-react'

import {
  listGenes,
  getGeneDetail,
  getGeneEvolutionTree,
  retireGene,
  reactivateGene,
  type GeneSummary,
  type GeneDetail,
  type GeneEvolutionTree,
} from '@/lib/tauri-bridge'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { ModuleHeader } from '../../shared/ModuleHeader'

type DetailTab = 'detail' | 'capsules' | 'tree'

function StatusBadge({ status }: { status: string }): React.ReactElement {
  const color =
    status === 'Active'
      ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
      : status === 'Retired'
        ? 'bg-amber-500/15 text-amber-600 dark:text-amber-400'
        : 'bg-muted text-muted-foreground'
  return (
    <span
      className={cn(
        'inline-block rounded px-1.5 py-0.5 text-[10px] font-medium tabular-nums',
        color,
      )}
    >
      {status === 'Active' ? '活跃' : status === 'Retired' ? '退役' : status}
    </span>
  )
}

function CategoryBadge({ category }: { category: string }): React.ReactElement {
  return (
    <span className="inline-block rounded bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">
      {category}
    </span>
  )
}

export function EvolutionModule(): React.ReactElement {
  const [genes, setGenes] = React.useState<GeneSummary[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [statusFilter, setStatusFilter] = React.useState<string>('active')
  const [activeTab, setActiveTab] = React.useState<DetailTab>('detail')
  const [query, setQuery] = React.useState('')
  const [geneDetail, setGeneDetail] = React.useState<GeneDetail | null>(null)
  const [detailLoading, setDetailLoading] = React.useState(false)
  // Load gene list
  const loadGenes = React.useCallback(() => {
    setLoading(true)
    listGenes(statusFilter === 'all' ? undefined : statusFilter)
      .then(setGenes)
      .catch((e) => {
        console.error('[EvolutionModule] listGenes failed', e)
        toast.error('加载 Gene 列表失败')
      })
      .finally(() => setLoading(false))
  }, [statusFilter])

  React.useEffect(() => {
    loadGenes()
  }, [loadGenes])

  // Load detail when selected
  React.useEffect(() => {
    if (!selectedId) {
      setGeneDetail(null)
      return
    }
    setDetailLoading(true)
    getGeneDetail(selectedId)
      .then(setGeneDetail)
      .catch((e) => console.error('[EvolutionModule] getGeneDetail failed', e))
      .finally(() => setDetailLoading(false))
  }, [selectedId])

  // Handle retire
  const handleRetire = async (assetId: string) => {
    try {
      await retireGene(assetId, '手动退役')
      toast.success('Gene 已退役')
      loadGenes()
      if (selectedId === assetId) setSelectedId(null)
    } catch (e) {
      toast.error('退役失败')
      console.error(e)
    }
  }

  // Handle reactivate
  const handleReactivate = async (assetId: string) => {
    try {
      await reactivateGene(assetId)
      toast.success('Gene 已重新激活')
      loadGenes()
    } catch (e) {
      toast.error('激活失败')
      console.error(e)
    }
  }

  const filtered = genes.filter((g) => {
    if (!query.trim()) return true
    const q = query.toLowerCase()
    return (
      g.gene_id.toLowerCase().includes(q) ||
      g.summary.toLowerCase().includes(q) ||
      g.category.toLowerCase().includes(q)
    )
  })

  const selected = genes.find((g) => g.asset_id === selectedId)

  return (
    <div className="flex flex-col h-full">
      <ModuleHeader
        group="capability"
        title="进化"
        subtitle="GEP Gene 自进化引擎"
        actions={
          <div className="flex gap-1">
            {(['all', 'active', 'Retired'] as const).map((f) => (
              <button
                key={f}
                type="button"
                onClick={() => setStatusFilter(f)}
                className={cn(
                  'rounded px-2 py-0.5 text-[10.5px] transition-colors',
                  statusFilter === f
                    ? 'bg-foreground/10 text-foreground font-medium'
                    : 'text-muted-foreground hover:bg-muted/50',
                )}
              >
                {f === 'all' ? '全部' : f === 'active' ? '活跃' : '退役'}
              </button>
            ))}
          </div>
        }
      />

      <div className="flex flex-1 min-h-0">
        {/* Left: Gene list */}
        <div className="w-56 flex-shrink-0 border-r border-border/40 flex flex-col">
          <div className="px-2 py-2">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/60" />
              <Input
                placeholder="搜索 Gene…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="h-7 pl-7 text-[11.5px]"
              />
            </div>
          </div>

          <div className="flex-1 overflow-auto">
            {loading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground/60" />
              </div>
            ) : filtered.length === 0 ? (
              <div className="p-4 text-center text-[11.5px] text-muted-foreground/60">
                尚无 Gene 记录
              </div>
            ) : (
              filtered.map((g) => (
                <button
                  key={g.asset_id}
                  type="button"
                  onClick={() => setSelectedId(g.asset_id)}
                  className={cn(
                    'w-full text-left px-3 py-2 border-b border-border/20 transition-colors',
                    selectedId === g.asset_id
                      ? 'bg-muted/60'
                      : 'hover:bg-muted/30',
                  )}
                >
                  <div className="flex items-center gap-1.5 mb-0.5">
                    <StatusBadge status={g.status} />
                    <CategoryBadge category={g.category} />
                    <span className="text-[10px] text-muted-foreground/60 ml-auto">
                      {g.capsule_count}
                    </span>
                  </div>
                  <div className="text-[11.5px] font-medium truncate">{g.gene_id}</div>
                  <div className="text-[10.5px] text-muted-foreground/80 truncate mt-0.5">
                    {g.summary}
                  </div>
                </button>
              ))
            )}
          </div>
        </div>

        {/* Right: Detail panels */}
        <div className="flex-1 flex flex-col min-w-0">
          {!selected ? (
            <div className="flex-1 flex items-center justify-center text-[12px] text-muted-foreground/60">
              ← 选择一个 Gene 查看详情
            </div>
          ) : (
            <>
              {/* Tab bar */}
              <div className="flex gap-0 border-b border-border/40 px-3">
                {([
                  ['detail', '详情'],
                  ['capsules', 'Capsules'],
                  ['tree', '版本树'],
                ] as const).map(([k, label]) => (
                  <button
                    key={k}
                    type="button"
                    onClick={() => setActiveTab(k)}
                    className={cn(
                      'px-3 py-1.5 text-[11.5px] border-b-2 transition-colors',
                      activeTab === k
                        ? 'border-foreground text-foreground font-medium'
                        : 'border-transparent text-muted-foreground hover:text-foreground',
                    )}
                  >
                    {label}
                  </button>
                ))}
              </div>

              {/* Content */}
              <div className="flex-1 overflow-auto p-3">
                {detailLoading ? (
                  <div className="flex items-center justify-center py-8">
                    <Loader2 className="h-4 w-4 animate-spin text-muted-foreground/60" />
                  </div>
                ) : (
                  <>
                    {activeTab === 'detail' && geneDetail && (
                      <GeneDetailView
                        detail={geneDetail}
                        onRetire={() => handleRetire(selected.asset_id)}
                        onReactivate={() => handleReactivate(selected.asset_id)}
                      />
                    )}
                    {activeTab === 'capsules' && geneDetail && (
                      <CapsuleTimeline capsules={geneDetail.capsules} />
                    )}
                    {activeTab === 'tree' && (
                      <GeneTreeView geneId={selected.gene_id} />
                    )}
                  </>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}

/** Six-tuple detail view */
function GeneDetailView({
  detail,
  onRetire,
  onReactivate,
}: {
  detail: GeneDetail
  onRetire: () => void
  onReactivate: () => void
}): React.ReactElement {
  const { gene } = detail
  const isActive = gene.status === 'Active'

  return (
    <div className="space-y-3 text-[12px] max-w-2xl">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-[14px] font-semibold">{gene.gene_id}</h3>
          <div className="flex items-center gap-1.5 mt-0.5">
            <StatusBadge status={gene.status} />
            <CategoryBadge category={gene.category} />
            <span className="text-[10.5px] text-muted-foreground">v{gene.version}</span>
          </div>
        </div>
        <Button
          size="sm"
          variant={isActive ? 'destructive' : 'outline'}
          onClick={isActive ? onRetire : onReactivate}
          className="h-7 text-[11px]"
        >
          {isActive ? (
            <>
              <Trash2 className="h-3 w-3 mr-1" />
              退役
            </>
          ) : (
            <>
              <RotateCcw className="h-3 w-3 mr-1" />
              激活
            </>
          )}
        </Button>
      </div>

      {/* Six-tuple cards */}
      <TupleCard label="μ · 匹配信号" items={gene.signals_match} color="border-blue-500/30" bg="bg-blue-50/30 dark:bg-blue-950/20" />
      <TupleCard label="π · 策略" items={gene.strategy} color="border-emerald-500/30" bg="bg-emerald-50/30 dark:bg-emerald-950/20" />
      <TupleCard label="α · 规避" items={gene.avoid} color="border-red-500/30" bg="bg-red-50/30 dark:bg-red-950/20" />
      <TupleCard label="c · 约束" items={[`最多 ${gene.constraints.max_files} 文件`, `禁止路径: ${gene.constraints.forbidden_paths.join(', ') || '无'}`]} color="border-amber-500/30" bg="bg-amber-50/30 dark:bg-amber-950/20" />
      <TupleCard label="v · 验证" items={[gene.validation]} color="border-purple-500/30" bg="bg-purple-50/30 dark:bg-purple-950/20" />

      <div className="border rounded-md p-3 border-muted-foreground/20 bg-muted/10">
        <div className="text-[10.5px] font-medium text-muted-foreground/80 mb-1">摘要</div>
        <div className="text-[12px]">{gene.summary}</div>
        <div className="mt-2 flex gap-3 text-[10px] text-muted-foreground/60">
          <span>asset: {gene.asset_id.slice(0, 12)}…</span>
          <span>创建: {new Date(gene.created_at).toLocaleDateString()}</span>
        </div>
      </div>
    </div>
  )
}

function TupleCard({
  label,
  items,
  color,
  bg,
}: {
  label: string
  items: string[]
  color: string
  bg: string
}): React.ReactElement {
  return (
    <div className={cn('border rounded-md p-2.5', color, bg)}>
      <div className="text-[10.5px] font-semibold text-muted-foreground/80 mb-1">{label}</div>
      {items.length === 0 ? (
        <div className="text-[11px] text-muted-foreground/50 italic">无</div>
      ) : (
        <ul className="space-y-0.5">
          {items.map((item, i) => (
            <li key={i} className="text-[11.5px] flex gap-1.5">
              <span className="text-muted-foreground/40 flex-shrink-0">•</span>
              <span>{item}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Capsule timeline */
function CapsuleTimeline({ capsules }: { capsules: GeneDetail['capsules'] }): React.ReactElement {
  if (capsules.length === 0) {
    return (
      <div className="text-center py-8 text-[12px] text-muted-foreground/60">
        尚无 Capsule 记录
      </div>
    )
  }

  return (
    <div className="space-y-1 max-w-xl">
      {capsules.map((c, i) => {
        const isSuccess = c.outcome.status === 'success'
        return (
          <div key={c.id || i} className="flex gap-2 py-2 border-b border-border/20 last:border-0">
            <div className="flex-shrink-0 mt-0.5">
              <div
                className={cn(
                  'w-2 h-2 rounded-full',
                  isSuccess ? 'bg-emerald-500' : c.outcome.status === 'failed' ? 'bg-red-500' : 'bg-amber-500',
                )}
              />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 mb-0.5">
                <span className="text-[10.5px] font-medium">{c.summary || c.id}</span>
                <span
                  className={cn(
                    'text-[10px]',
                    isSuccess ? 'text-emerald-600' : 'text-red-600',
                  )}
                >
                  {c.outcome.status === 'success' ? '成功' : c.outcome.status === 'failed' ? '失败' : '部分'}
                  {' · '}
                  {Math.round(c.confidence * 100)}%
                </span>
              </div>
              <div className="text-[10px] text-muted-foreground/60 flex gap-2">
                <span>文件: {c.blast_radius.files}</span>
                <span>行: {c.blast_radius.lines}</span>
                <span>streak: {c.effective_streak.toFixed(1)}</span>
                <span>{new Date(c.created_at).toLocaleDateString()}</span>
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}

/** Version tree */
function GeneTreeView({ geneId }: { geneId: string }): React.ReactElement {
  const [tree, setTree] = React.useState<GeneEvolutionTree | null>(null)
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    setLoading(true)
    getGeneEvolutionTree(geneId)
      .then(setTree)
      .catch((e) => console.error('[GeneTreeView]', e))
      .finally(() => setLoading(false))
  }, [geneId])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground/60" />
      </div>
    )
  }

  if (!tree || tree.versions.length === 0) {
    return (
      <div className="text-center py-8 text-[12px] text-muted-foreground/60">
        尚无版本记录
      </div>
    )
  }

  return (
    <div className="max-w-xl space-y-2">
      <h4 className="text-[12px] font-semibold mb-2">版本历史 · {tree.versions.length} 个版本</h4>
      {tree.versions.map((v, i) => (
        <div key={v.asset_id} className="flex items-start gap-3 py-1.5 border-b border-border/20 last:border-0">
          <div className="flex-shrink-0 w-6 text-center">
            <span className="text-[10.5px] font-mono text-muted-foreground/60">v{i + 1}</span>
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-medium">{v.version}</span>
              {v.parent_asset_id && (
                <span className="text-[10px] text-muted-foreground/60">
                  ← {v.parent_asset_id.slice(0, 8)}…
                </span>
              )}
            </div>
            <div className="text-[10.5px] text-muted-foreground/80 mt-0.5">{v.summary}</div>
            <div className="text-[10px] text-muted-foreground/60 mt-0.5">
              {new Date(v.created_at).toLocaleDateString()}
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}

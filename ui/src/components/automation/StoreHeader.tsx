import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { Search, RotateCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  marketplaceFiltersAtom,
  marketplaceCategoryCountsAtom,
  marketplaceLoadingAtom,
  type MarketplaceItemTypeFilter,
} from '@/atoms/marketplace'

const TYPE_TABS: { id: MarketplaceItemTypeFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'automation', label: '数字人' },
  { id: 'skill', label: '技能' },
  { id: 'mcp', label: 'MCP' },
]

const CATEGORY_LABELS: Record<string, string> = {
  social: '社交',
  productivity: '生产力',
  content: '内容',
  news: '新闻',
  data: '数据',
  dev: '开发',
  shopping: '购物',
  other: '其他',
}

interface Props {
  onRefresh: () => void
}

export function StoreHeader({ onRefresh }: Props): React.ReactElement {
  const [filters, setFilters] = useAtom(marketplaceFiltersAtom)
  const counts = useAtomValue(marketplaceCategoryCountsAtom)
  const loading = useAtomValue(marketplaceLoadingAtom)

  // Debounce search by 300ms — hold a draft string in local state and push to atom on settle.
  const [draft, setDraft] = React.useState(filters.search)
  React.useEffect(() => {
    if (draft === filters.search) return
    const handle = setTimeout(() => {
      setFilters((f) => ({ ...f, search: draft }))
    }, 300)
    return () => clearTimeout(handle)
  }, [draft, filters.search, setFilters])

  // Build the category chip list — show only categories with at least one item
  const categoryChips = React.useMemo(() => {
    return Object.entries(counts).sort((a, b) => b[1] - a[1])
  }, [counts])

  return (
    <div className="border-b border-border/50">
      {/* Row 1: search + refresh */}
      <div className="flex items-center gap-2 px-6 py-3">
        <div className="relative flex-1 max-w-2xl">
          <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/60" />
          <input
            type="text"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="搜索数字人 / 技能 / MCP..."
            className={cn(
              'w-full pl-8 pr-3 py-1.5 text-[13px]',
              'rounded-md border border-border/50 bg-card',
              'placeholder:text-muted-foreground/50',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
              'transition-colors',
            )}
          />
        </div>
        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className={cn(
            'p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/30 transition-colors',
            loading && 'opacity-50 cursor-wait',
          )}
          title="刷新注册表"
        >
          <RotateCw size={13} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>

      {/* Row 2: type tabs */}
      <div className="flex items-center gap-1 px-6 pb-2">
        {TYPE_TABS.map((tab) => {
          const active = filters.itemType === tab.id
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => setFilters((f) => ({ ...f, itemType: tab.id, category: null }))}
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

      {/* Row 3: category chips (only when there are any) */}
      {categoryChips.length > 0 && (
        <div className="flex items-center gap-1.5 px-6 pb-3 overflow-x-auto">
          <button
            type="button"
            onClick={() => setFilters((f) => ({ ...f, category: null }))}
            className={cn(
              'shrink-0 px-2 py-0.5 rounded-full text-[11px] border transition-colors',
              filters.category === null
                ? 'bg-primary/10 text-primary border-primary/30'
                : 'bg-muted text-muted-foreground border-border/50 hover:bg-muted/80',
            )}
          >
            全部
          </button>
          {categoryChips.map(([cat, count]) => {
            const active = filters.category === cat
            const label = CATEGORY_LABELS[cat] ?? cat
            return (
              <button
                key={cat}
                type="button"
                onClick={() => setFilters((f) => ({ ...f, category: cat }))}
                className={cn(
                  'shrink-0 px-2 py-0.5 rounded-full text-[11px] border transition-colors tabular-nums',
                  active
                    ? 'bg-primary/10 text-primary border-primary/30'
                    : 'bg-muted text-muted-foreground border-border/50 hover:bg-muted/80',
                )}
              >
                {label} · {count}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}

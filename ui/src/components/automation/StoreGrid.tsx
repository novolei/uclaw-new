import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Loader2, Search as SearchIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { StoreCard } from './StoreCard'
import {
  marketplaceItemsAtom,
  marketplaceLoadingAtom,
  marketplaceLoadErrorAtom,
  marketplaceHasMoreAtom,
  marketplaceTotalAtom,
  marketplaceUpdatesAtom,
  marketplaceSelectedSlugAtom,
  automationsSubviewAtom,
} from '@/atoms/marketplace'
import { humaneSpecsAtom } from '@/atoms/automation'

interface Props {
  onLoadMore: () => void
}

export function StoreGrid({ onLoadMore }: Props): React.ReactElement {
  const items = useAtomValue(marketplaceItemsAtom)
  const loading = useAtomValue(marketplaceLoadingAtom)
  const error = useAtomValue(marketplaceLoadErrorAtom)
  const hasMore = useAtomValue(marketplaceHasMoreAtom)
  const total = useAtomValue(marketplaceTotalAtom)
  const updates = useAtomValue(marketplaceUpdatesAtom)
  const installedSpecs = useAtomValue(humaneSpecsAtom)
  const setSelectedSlug = useSetAtom(marketplaceSelectedSlugAtom)
  const setSubview = useSetAtom(automationsSubviewAtom)

  const updateSlugs = React.useMemo(() => new Set(updates.map((u) => u.slug)), [updates])
  const installedSlugs = React.useMemo(() => {
    return new Set(
      installedSpecs
        .filter((s) => s.source === 'marketplace' && s.sourceRef)
        .map((s) => {
          // source_ref shape: 'marketplace://halo/{slug}'
          const m = /^marketplace:\/\/[^/]+\/(.+)$/.exec(s.sourceRef ?? '')
          return m?.[1] ?? null
        })
        .filter((x): x is string => x !== null),
    )
  }, [installedSpecs])

  const openDetail = (slug: string) => {
    setSelectedSlug(slug)
    setSubview('store-detail')
  }

  // Empty / loading / error states
  if (error) {
    return (
      <div className="flex flex-col items-center gap-3 py-16 text-muted-foreground">
        <span className="text-[13px]">无法加载市场</span>
        <span className="text-[11px] max-w-md text-center">{error}</span>
      </div>
    )
  }
  if (loading && items.length === 0) {
    return (
      <div className="flex items-center gap-2 justify-center py-16 text-muted-foreground">
        <Loader2 size={14} className="animate-spin" />
        <span className="text-[13px]">正在加载注册表...</span>
      </div>
    )
  }
  if (!loading && items.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 py-16 text-muted-foreground">
        <SearchIcon size={28} className="text-muted-foreground/30" />
        <p className="text-[13px]">市场里还没有匹配的数字员工</p>
        <p className="text-[11px]">试试别的关键词，或浏览全部分类</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col">
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 px-6 py-4">
        {items.map((item) => (
          <StoreCard
            key={item.slug}
            item={item}
            hasUpdate={updateSlugs.has(item.slug)}
            isInstalled={installedSlugs.has(item.slug)}
            onClick={openDetail}
          />
        ))}
      </div>
      {hasMore && (
        <div className="flex justify-center pb-6">
          <button
            type="button"
            onClick={onLoadMore}
            disabled={loading}
            className={cn(
              'px-4 py-1.5 text-[12px] rounded-md',
              'border border-border/50 bg-card hover:bg-accent/30',
              'transition-colors disabled:opacity-50 disabled:cursor-wait',
            )}
          >
            {loading ? '加载中...' : `加载更多（已显示 ${items.length} / ${total}）`}
          </button>
        </div>
      )}
    </div>
  )
}

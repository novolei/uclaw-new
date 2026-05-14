import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import { StoreHeader } from './StoreHeader'
import { StoreFeaturedRow } from './StoreFeaturedRow'
import { StoreGrid } from './StoreGrid'
import {
  marketplaceItemsAtom,
  marketplacePageAtom,
  marketplaceHasMoreAtom,
  marketplaceTotalAtom,
  marketplaceLoadingAtom,
  marketplaceLoadErrorAtom,
  marketplaceFiltersAtom,
  marketplaceCategoryCountsAtom,
  marketplaceUpdatesAtom,
} from '@/atoms/marketplace'
import {
  queryMarketplace,
  refreshMarketplace,
  checkMarketplaceUpdates,
  marketplaceCategoryCounts,
} from '@/lib/tauri-bridge'

const PAGE_SIZE = 20

export function StoreView(): React.ReactElement {
  const [, setItems] = useAtom(marketplaceItemsAtom)
  const [page, setPage] = useAtom(marketplacePageAtom)
  const setHasMore = useSetAtom(marketplaceHasMoreAtom)
  const setTotal = useSetAtom(marketplaceTotalAtom)
  const [loading, setLoading] = useAtom(marketplaceLoadingAtom)
  const setLoadError = useSetAtom(marketplaceLoadErrorAtom)
  const filters = useAtomValue(marketplaceFiltersAtom)
  const setCounts = useSetAtom(marketplaceCategoryCountsAtom)
  const setUpdates = useSetAtom(marketplaceUpdatesAtom)

  const loadPage = React.useCallback(
    async (pageNum: number, replace: boolean) => {
      setLoading(true)
      setLoadError(null)
      try {
        const result = await queryMarketplace(
          filters.search || undefined,
          filters.itemType === 'all' ? undefined : filters.itemType,
          filters.category ?? undefined,
          pageNum,
          PAGE_SIZE,
        )
        setItems((prev) => (replace ? result.items : [...prev, ...result.items]))
        setHasMore(result.hasMore)
        setTotal(result.total)
        setPage(pageNum)
      } catch (err) {
        setLoadError(String(err))
      } finally {
        setLoading(false)
      }
    },
    [filters, setItems, setHasMore, setTotal, setPage, setLoading, setLoadError],
  )

  // Reload when filters change
  React.useEffect(() => {
    void loadPage(0, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filters.search, filters.itemType, filters.category])

  // Category chip counts: aggregate the entire post-itemType+post-search corpus
  // server-side. Independent of `filters.category` on purpose — that's what
  // makes the chip count + ordering stable across category clicks.
  React.useEffect(() => {
    marketplaceCategoryCounts(
      filters.itemType === 'all' ? undefined : filters.itemType,
      filters.search || undefined,
    )
      .then(setCounts)
      .catch((err) => console.warn('[StoreView] category counts failed:', err))
  }, [filters.itemType, filters.search, setCounts])

  // Initial updates check
  React.useEffect(() => {
    checkMarketplaceUpdates()
      .then(setUpdates)
      .catch((err) => console.warn('[StoreView] check updates failed:', err))
  }, [setUpdates])

  const handleLoadMore = () => {
    if (!loading) void loadPage(page + 1, false)
  }

  const handleRefresh = async () => {
    try {
      const count = await refreshMarketplace()
      toast.success(`已刷新，${count} 个项目`)
      void loadPage(0, true)
    } catch (err) {
      toast.error(`刷新失败：${String(err)}`)
    }
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <StoreHeader onRefresh={handleRefresh} />
      <div className="flex-1 overflow-y-auto">
        <StoreFeaturedRow />
        <StoreGrid onLoadMore={handleLoadMore} />
      </div>
    </div>
  )
}

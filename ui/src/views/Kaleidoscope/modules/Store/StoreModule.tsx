/**
 * StoreModule — 万花筒「应用商店」模块。
 *
 * 渲染现有 StoreView；当 automationsSubviewAtom 进入 'store-detail' 时渲染
 * StoreDetail（复刻原 AutomationsView 的 store/store-detail 切换）。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { automationsSubviewAtom, marketplaceSelectedSlugAtom } from '@/atoms/marketplace'
import { StoreView } from '@/components/automation/StoreView'
import { StoreDetail } from '@/components/automation/StoreDetail'

export function StoreModule(): React.ReactElement {
  const subview = useAtomValue(automationsSubviewAtom)
  const selectedSlug = useAtomValue(marketplaceSelectedSlugAtom)
  // automationsSubviewAtom is persisted (atomWithStorage) but the selected
  // slug is not — a restart can leave subview === 'store-detail' with no slug,
  // which strands StoreDetail on an unbreakable "正在加载详情..." spinner
  // (its load effect early-returns on !slug). Only render StoreDetail when a
  // slug is actually selected; otherwise fall back to the store grid.
  const showDetail = subview === 'store-detail' && selectedSlug !== null
  return (
    <div className="absolute inset-0">
      {showDetail ? <StoreDetail /> : <StoreView />}
    </div>
  )
}

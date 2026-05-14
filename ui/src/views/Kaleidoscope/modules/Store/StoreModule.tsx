/**
 * StoreModule — 万花筒「应用商店」模块。
 *
 * 渲染现有 StoreView；当 automationsSubviewAtom 进入 'store-detail' 时渲染
 * StoreDetail（复刻原 AutomationsView 的 store/store-detail 切换）。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { automationsSubviewAtom } from '@/atoms/marketplace'
import { StoreView } from '@/components/automation/StoreView'
import { StoreDetail } from '@/components/automation/StoreDetail'

export function StoreModule(): React.ReactElement {
  const subview = useAtomValue(automationsSubviewAtom)
  return (
    <div className="absolute inset-0">
      {subview === 'store-detail' ? <StoreDetail /> : <StoreView />}
    </div>
  )
}

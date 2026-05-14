/**
 * ComingSoonModule — Phase 1 占位。
 *
 * 万花筒 Rail 在 Phase 1 就展示全部 7 个模块入口（导航结构是骨架的一部分），
 * 但只有「数字人」有真实实现。其余 6 个点进来渲染这个占位，Phase 2 逐个替换。
 */
import * as React from 'react'
import { KALEIDOSCOPE_MODULES, type KaleidoscopeModuleId } from '@/atoms/kaleidoscope'
import { ModuleHeader } from '../shared/ModuleHeader'

export interface ComingSoonModuleProps {
  moduleId: KaleidoscopeModuleId
}

export function ComingSoonModule({
  moduleId,
}: ComingSoonModuleProps): React.ReactElement {
  const meta = KALEIDOSCOPE_MODULES.find((m) => m.id === moduleId)
  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader group={meta?.group ?? 'asset'} title={meta?.label ?? '模块'} />
      <div className="flex-1 min-h-0 flex items-center justify-center">
        <div className="text-[13px] text-muted-foreground">即将到来 · Phase 2</div>
      </div>
    </div>
  )
}

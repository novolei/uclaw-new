/**
 * HumansModule — 万花筒「数字人」模块。
 *
 * Phase 1：直接复用现有 AutomationHub（AutomationsView 的 'humans' 子视图），
 * 套一层统一的 ModuleHeader。Phase 2 再按画廊规格细化卡片与详情抽屉。
 */
import * as React from 'react'
import { AutomationHub } from '@/components/automation/AutomationHub'
import { ModuleHeader } from '../../shared/ModuleHeader'

export function HumansModule(): React.ReactElement {
  return (
    <div className="flex flex-col h-full min-h-0">
      <ModuleHeader group="asset" title="数字人 · Automations" />
      <div className="flex-1 min-h-0">
        <AutomationHub />
      </div>
    </div>
  )
}

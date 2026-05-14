/**
 * HumansModule — 万花筒「数字人」模块。
 *
 * 渲染现有 AutomationHub（原 AutomationsView 的 'humans' 子视图）。
 * AutomationHub 根是 h-full，需确定高度的父容器 —— 用 absolute inset-0
 * （KaleidoscopeShell 的主区卡片是 relative）。AutomationHub 自带 header，
 * 不再叠 ModuleHeader。
 */
import * as React from 'react'
import { AutomationHub } from '@/components/automation/AutomationHub'

export function HumansModule(): React.ReactElement {
  return (
    <div className="absolute inset-0">
      <AutomationHub />
    </div>
  )
}

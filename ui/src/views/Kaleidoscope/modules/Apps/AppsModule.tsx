/**
 * AppsModule — 万花筒「我的应用」模块。
 *
 * 渲染现有 AppsTab（原 AutomationsView 的 'apps' 子视图）。
 */
import * as React from 'react'
import { AppsTab } from '@/components/automation/AppsTab'

export function AppsModule(): React.ReactElement {
  return (
    <div className="absolute inset-0">
      <AppsTab />
    </div>
  )
}

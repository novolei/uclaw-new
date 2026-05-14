/**
 * MainArea — 主内容区域
 *
 * 顶层 surface 切换：'workspace'（任务流，WorkspaceShell）↔ 'kaleidoscope'
 * （配置流，KaleidoscopeShell）。两个 surface 用 motion 的 AnimatePresence
 * 做 200ms cross-dissolve。设置以浮窗形式叠加显示。
 *
 * Task 8 会把 topLevelViewAtom 的 switch 接进来；本次（Task 2）只渲染
 * WorkspaceShell，等价于重构前行为。
 */

import * as React from 'react'
import { Panel } from '@/components/app-shell/Panel'
import { SettingsDialog } from '@/components/settings/SettingsDialog'
import { WorkspaceShell } from '@/views/Workspace/WorkspaceShell'

export function MainArea(): React.ReactElement {
  return (
    <>
      <Panel
        variant="grow"
        className="bg-content-area rounded-2xl shadow-xl"
      >
        <WorkspaceShell />
      </Panel>
      <SettingsDialog />
    </>
  )
}

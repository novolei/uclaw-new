/**
 * MainArea — workspace surface 的主内容区域。
 *
 * 顶层 surface 切换（workspace ↔ kaleidoscope）在 AppShell 层完成 —— 见
 * AppShell.tsx。MainArea 只负责 workspace surface 自己的内容。
 */

import * as React from 'react'
import { Panel } from '@/components/app-shell/Panel'
import { WorkspaceShell } from '@/views/Workspace/WorkspaceShell'

export function MainArea(): React.ReactElement {
  return (
    <Panel
      variant="grow"
      className="bg-content-area rounded-2xl shadow-xl"
    >
      <WorkspaceShell />
    </Panel>
  )
}

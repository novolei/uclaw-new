/**
 * AppShell - 应用主布局容器
 *
 * 布局结构：[LeftSidebar 可折叠] | [MainArea: TabBar + TabContent] | [RightSidePanel 可折叠]
 *
 * MainArea 支持多标签页，Settings 视图为独立覆盖。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { LeftSidebar } from './LeftSidebar'
import { RightSidePanel } from './RightSidePanel'
import { MainArea } from '@/components/tabs/MainArea'
import { AppShellProvider, type AppShellContextType } from '@/contexts/AppShellContext'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom, currentSessionSidePanelOpenAtom } from '@/atoms/agent-atoms'
import { cn } from '@/lib/utils'

export interface AppShellProps {
  /** Context 值，用于传递给子组件 */
  contextValue: AppShellContextType
}

export function AppShell({ contextValue }: AppShellProps): React.ReactElement {
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const isPanelOpen = useAtomValue(currentSessionSidePanelOpenAtom)
  const showRightPanel = appMode === 'agent' && !!currentSessionId

  return (
    <AppShellProvider value={contextValue}>
      <div className="shell-bg h-screen w-screen flex overflow-hidden bg-gradient-to-br from-zinc-50 to-zinc-100 dark:from-zinc-950 dark:to-zinc-900">
        {/* 左侧边栏：可折叠，带圆角和内边距 */}
        <div className="p-2 pr-0 relative">
          <LeftSidebar />
        </div>

        {/* 中间容器：主内容区域 */}
        <div className="main-panel flex-1 min-w-0 p-2 relative">
          {/* 主题背景图层（仅特殊主题如 THE FINALS 使用，其他主题下为空） */}
          <div aria-hidden="true" className="main-panel-bg pointer-events-none absolute inset-0 z-0" />
          {/* 主内容区域（TabBar + TabContent） */}
          <div className="relative z-10 flex flex-col h-full min-h-0 min-w-0">
            <MainArea />
          </div>
        </div>

        {/* 右侧边栏：Agent 文件面板，带圆角和内边距 */}
        {showRightPanel && (
          <div className={cn('relative transition-[padding] duration-300 ease-in-out', isPanelOpen ? 'p-2 pl-0' : 'p-0')}>
            <RightSidePanel />
          </div>
        )}
      </div>
    </AppShellProvider>
  )
}

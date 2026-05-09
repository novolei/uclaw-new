/**
 * AppShell - 应用主布局容器
 *
 * 布局结构：[LeftSidebar 可折叠] | [MainArea: TabBar + TabContent] | [RightSidePanel 可折叠]
 *
 * MainArea 支持多标签页，Settings 视图为独立覆盖。
 */

import * as React from 'react'
import { useAtomValue, useAtom, useSetAtom } from 'jotai'
import { LeftSidebar } from './LeftSidebar'
import { RightSidePanel } from './RightSidePanel'
import { MainArea } from '@/components/tabs/MainArea'
import { AppShellProvider, type AppShellContextType } from '@/contexts/AppShellContext'
import { appModeAtom } from '@/atoms/app-mode'
import {
  agentSessionsAtom,
  currentAgentSessionIdAtom,
  currentAgentWorkspaceIdAtom,
  currentSessionSidePanelOpenAtom,
} from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { tabsAtom, activeTabIdAtom, openTab } from '@/atoms/tab-atoms'
import { SearchPalette } from '@/components/search/SearchPalette'
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

  // Tab navigation atoms — used by handleSearchResultSelect
  const [tabs, setTabs] = useAtom(tabsAtom)
  const setActiveTabId = useSetAtom(activeTabIdAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setCurrentConversationId = useSetAtom(currentConversationIdAtom)
  const setCurrentAgentSessionId = useSetAtom(currentAgentSessionIdAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const setCurrentAgentWorkspaceId = useSetAtom(currentAgentWorkspaceIdAtom)

  const handleSearchResultSelect = React.useCallback((r: {
    source: 'conversation' | 'chat_message' | 'agent_turn' | 'file'
    sourceId: string
    messageId?: string
  }) => {
    // Determine tab type from source — chat_message → chat, agent_turn → agent,
    // conversation matches whichever type is already open (prefer existing tab).
    const existingTab = tabs.find((t) => t.sessionId === r.sourceId)
    const tabType = existingTab?.type
      ?? (r.source === 'agent_turn' ? 'agent' : 'chat')

    const result = openTab(tabs, {
      type: tabType,
      sessionId: r.sourceId,
      title: existingTab?.title ?? '',
    })
    setTabs(result.tabs)
    setActiveTabId(result.activeTabId)

    // Sync sidebar state — mirrors TabBar.handleActivate
    if (tabType === 'chat') {
      setAppMode('chat')
      setCurrentConversationId(r.sourceId)
    } else if (tabType === 'agent') {
      setAppMode('agent')
      setCurrentAgentSessionId(r.sourceId)
      const session = agentSessions.find((s) => s.id === r.sourceId)
      if (session?.workspaceId) {
        setCurrentAgentWorkspaceId(session.workspaceId)
        localStorage.setItem(`uclaw:workspace:${r.sourceId}`, session.workspaceId)
      }
    }

    // After a short paint delay, scroll to the specific message inside the session.
    if (r.messageId) {
      setTimeout(() => {
        window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', {
          detail: { sessionId: r.sourceId, messageId: r.messageId },
        }))
      }, 200)
    }
  }, [tabs, setTabs, setActiveTabId, setAppMode, setCurrentConversationId, setCurrentAgentSessionId, agentSessions, setCurrentAgentWorkspaceId])

  return (
    <AppShellProvider value={contextValue}>
      <div className="shell-bg h-screen w-screen flex overflow-hidden bg-gradient-to-br from-zinc-50 to-zinc-100 dark:from-zinc-950 dark:to-zinc-900">
        {/* 左侧边栏：可折叠，带圆角和内边距 */}
        <div className="sidebar-wrapper p-2 pr-0 relative">
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
          <div className={cn('right-panel-wrapper relative transition-[padding] duration-300 ease-in-out', isPanelOpen ? 'p-2 pl-0' : 'p-0')}>
            <RightSidePanel />
          </div>
        )}

        {/* Global ⌘K search palette — mounts at root so it works from any view */}
        <SearchPalette onSelect={handleSearchResultSelect} />
      </div>
    </AppShellProvider>
  )
}

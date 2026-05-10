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
import { ApprovalModal } from '@/components/ApprovalModal'
import { ModeBanner } from '@/components/agent/ModeBanner'
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
import { installScrollToMessage } from '@/lib/scroll-to-message'

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

  React.useEffect(() => {
    const dispose = installScrollToMessage()
    return dispose
  }, [])

  const handleSearchResultSelect = React.useCallback((payload:
    | { kind: 'thread'; thread: { id: string; kind: 'chat' | 'agent'; workspaceId: string } }
    | { kind: 'workspace'; workspace: { id: string; name: string } }
    | { kind: 'settings'; settings: { id: string } }
    | { kind: 'search_hit'; hit: { source: string; sourceId: string; messageId?: string } }
  ) => {
    switch (payload.kind) {
      case 'thread': {
        const t = payload.thread
        // Open the right tab type — chat or agent
        const tabType = t.kind === 'agent' ? 'agent' : 'chat'
        const result = openTab(tabs, { type: tabType, sessionId: t.id, title: '' })
        setTabs(result.tabs)
        setActiveTabId(result.activeTabId)
        // Update the per-domain "current" atoms so the view focuses correctly.
        setAppMode(t.kind === 'agent' ? 'agent' : 'chat')
        if (t.kind === 'agent') setCurrentAgentSessionId(t.id)
        else setCurrentConversationId(t.id)
        setCurrentAgentWorkspaceId(t.workspaceId)
        break
      }
      case 'workspace': {
        // Switch to that workspace; don't open a thread automatically.
        setCurrentAgentWorkspaceId(payload.workspace.id)
        break
      }
      case 'settings': {
        // Navigate to settings tab. The settings tab id convention from existing code:
        setActiveTabId('settings')
        // TODO: Optionally pass a deep-link hint via a separate atom if one exists.
        // For now, just open the settings panel; the user can pick the right page.
        console.warn('Settings deep-link not yet implemented')
        break
      }
      case 'search_hit': {
        const h = payload.hit
        // existing PR #29 behavior — open the session and scroll to the message
        const tabType = (h.source === 'agent_turn' || h.source === 'agent_message') ? 'agent' : 'chat'
        const result = openTab(tabs, { type: tabType, sessionId: h.sourceId, title: '' })
        setTabs(result.tabs)
        setActiveTabId(result.activeTabId)
        setAppMode((h.source === 'agent_turn' || h.source === 'agent_message') ? 'agent' : 'chat')
        if ((h.source === 'agent_turn' || h.source === 'agent_message')) setCurrentAgentSessionId(h.sourceId)
        else setCurrentConversationId(h.sourceId)
        // Look up workspace from agent sessions if available
        const session = agentSessions.find((s) => s.id === h.sourceId)
        if (session?.workspaceId) {
          setCurrentAgentWorkspaceId(session.workspaceId)
          localStorage.setItem(`uclaw:workspace:${h.sourceId}`, session.workspaceId)
        }
        if (h.messageId) {
          setTimeout(() => {
            window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', {
              detail: { sessionId: h.sourceId, messageId: h.messageId },
            }))
          }, 200)
        }
        break
      }
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
            <ModeBanner />
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

        {/* Global tool-approval modal — listens for `agent:need_approval`
            IPC events. Must be mounted exactly once at the root, otherwise
            the agent loop's oneshot channel never resolves and the agent
            hangs forever waiting on the user. (This was missing for a long
            time; the bug was hidden because `bash` was in the global
            auto-approve whitelist short-circuiting the resolver.) */}
        <ApprovalModal />
      </div>
    </AppShellProvider>
  )
}

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
import { AskUserBanner } from '@/components/agent/AskUserBanner'
import { ExitPlanModeBanner } from '@/components/agent/ExitPlanModeBanner'
import { ModeBanner } from '@/components/agent/ModeBanner'
import { appModeAtom } from '@/atoms/app-mode'
import {
  agentSessionsAtom,
  allPendingAskUserRequestsAtom,
  allPendingExitPlanRequestsAtom,
  currentAgentSessionIdAtom,
  currentAgentWorkspaceIdAtom,
  currentSessionSidePanelOpenAtom,
  installAskUserListener,
  installExitPlanListener,
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
  const setAllPendingAskUserRequests = useSetAtom(allPendingAskUserRequestsAtom)
  const setAllPendingExitPlanRequests = useSetAtom(allPendingExitPlanRequestsAtom)

  React.useEffect(() => {
    const dispose = installScrollToMessage()
    return dispose
  }, [])

  React.useEffect(() => {
    let dispose: (() => void) | undefined
    installAskUserListener((updater) => setAllPendingAskUserRequests(updater)).then((d) => { dispose = d })
    return () => { dispose?.() }
  }, [setAllPendingAskUserRequests])

  React.useEffect(() => {
    let dispose: (() => void) | undefined
    installExitPlanListener((updater) => setAllPendingExitPlanRequests(updater)).then((d) => { dispose = d })
    return () => { dispose?.() }
  }, [setAllPendingExitPlanRequests])

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
      {/* macOS 全宽标题栏拖拽区（z-50）：覆盖窗口顶部 50px，让面板间隙也可拖动。
          各 panel 以 z-[60] 叠在其上，拖动只在 panel 之外的可见间隙生效。 */}
      <div className="titlebar-drag-region fixed top-0 left-0 right-0 h-[50px] z-50" />

      <div className="shell-bg h-screen w-screen flex overflow-hidden bg-gradient-to-br from-zinc-50 to-zinc-100 dark:from-zinc-950 dark:to-zinc-900">
        {/* 左侧边栏：可折叠，带圆角和内边距 */}
        <div className="sidebar-wrapper p-2 pr-0 relative z-[60]">
          <LeftSidebar />
        </div>

        {/* 中间容器：主内容区域。wrapper 自身可拖拽（8px padding 区域生效），
            内部 z-10 内容显式 no-drag 退出，TabBar 自身已有 drag class 自然覆盖回 drag。 */}
        <div data-tauri-drag-region className="main-panel titlebar-drag-region flex-1 min-w-0 p-2 relative z-[60]">
          {/* 主题背景图层（仅特殊主题如 THE FINALS 使用，其他主题下为空） */}
          <div aria-hidden="true" className="main-panel-bg pointer-events-none absolute inset-0 z-0" />
          {/* 主内容区域（TabBar + TabContent） */}
          <div className="titlebar-no-drag relative z-10 flex flex-col h-full min-h-0 min-w-0">
            <ModeBanner />
            <MainArea />
          </div>
        </div>

        {/* 右侧边栏：Agent 文件面板，带圆角和内边距 */}
        {showRightPanel && (
          <div className={cn('right-panel-wrapper relative z-[60] transition-[padding] duration-300 ease-in-out', isPanelOpen ? 'p-2 pl-0' : 'p-0')}>
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

        {/* Global ask_user banner — shows agent's question pending */}
        {currentSessionId && <AskUserBanner sessionId={currentSessionId} />}

        {/* Global exit_plan_mode banner — plan markdown + 3-decision modal */}
        {currentSessionId && <ExitPlanModeBanner sessionId={currentSessionId} />}
      </div>
    </AppShellProvider>
  )
}

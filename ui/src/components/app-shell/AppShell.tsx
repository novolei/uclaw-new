/**
 * AppShell - 应用主布局容器
 *
 * 布局结构：[LeftSidebar 可折叠] | [MainArea: TabBar + TabContent] | [RightSidePanel 可折叠]
 *
 * MainArea 支持多标签页，Settings 视图为独立覆盖。
 */

import * as React from 'react'
import { useAtomValue, useAtom, useSetAtom } from 'jotai'
import { getDefaultStore } from 'jotai'
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
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { SearchPalette } from '@/components/search/SearchPalette'
import { cn } from '@/lib/utils'
import { installScrollToMessage } from '@/lib/scroll-to-message'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { toast } from 'sonner'
import { attachWorkspaceDirectory, pathIsDirectory, copyFileIntoWorkspace } from '@/lib/tauri-bridge'
import { workspaceFilesVersionAtom, workspaceAttachedDirsMapAtom } from '@/atoms/agent-atoms'

export interface AppShellProps {
  /** Context 值，用于传递给子组件 */
  contextValue: AppShellContextType
}

export function AppShell({ contextValue }: AppShellProps): React.ReactElement {
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const isPanelOpen = useAtomValue(currentSessionSidePanelOpenAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
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

  // Phase 3: single window-level native drag-drop listener at app root.
  // Tabs each mount their own AgentView, but only one listener should
  // process OS drops. Routes folders → attach to active workspace;
  // files → copy bytes into active workspace folder.
  //
  // StrictMode safety: onDragDropEvent returns a Promise; if cleanup
  // runs before the Promise resolves (which happens on the first mount
  // in StrictMode), we'd miss the unlisten handle and end up with two
  // listeners stacked. The `cancelled` flag closes that gap.
  React.useEffect(() => {
    const win = getCurrentWindow()
    let cancelled = false
    let unlisten: (() => void) | undefined
    void win.onDragDropEvent(async (evt) => {
      const payload = evt.payload as { type?: string; paths?: string[] }
      if (payload.type !== 'drop' || !Array.isArray(payload.paths) || payload.paths.length === 0) return

      // Resolve current workspace at drop time (not at register time) so
      // workspace switching mid-session lands files in the right place.
      const ws = getDefaultStore().get(currentAgentWorkspaceIdAtom)
      if (!ws) {
        toast.error('请先选择工作区')
        return
      }

      const folderResults: string[][] = []
      for (const p of payload.paths) {
        try {
          const isDir = await pathIsDirectory(p)
          if (isDir) {
            const updated = await attachWorkspaceDirectory(ws, p)
            folderResults.push(updated)
            toast.success(`已附加目录: ${p}`)
          } else {
            const writtenPath = await copyFileIntoWorkspace(ws, p)
            const basename = writtenPath.split('/').pop() ?? writtenPath
            toast.success(`已上传文件: ${basename}`)
          }
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err)
          toast.error(`处理 ${p} 失败: ${msg}`)
        }
      }
      // Push the most-recent attached_dirs list to the atom so the
      // SidePanel 附加目录 section reflects the change without restart.
      if (folderResults.length > 0) {
        const latest = folderResults[folderResults.length - 1]
        getDefaultStore().set(workspaceAttachedDirsMapAtom, (prev) => {
          const m = new Map(prev)
          m.set(ws, latest)
          return m
        })
      }
      getDefaultStore().set(workspaceFilesVersionAtom, (v) => v + 1)
    }).then((u) => {
      // If the effect already cleaned up while the Promise was pending,
      // immediately remove the listener we just registered. Otherwise
      // keep the handle for the real cleanup.
      if (cancelled) {
        u()
      } else {
        unlisten = u
      }
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
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
        const ws = activeWorkspaceId ?? 'default'
        const result = openTab(tabs, { type: tabType, sessionId: t.id, title: '', workspaceId: ws })
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
        const ws = activeWorkspaceId ?? 'default'
        const result = openTab(tabs, { type: tabType, sessionId: h.sourceId, title: '', workspaceId: ws })
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

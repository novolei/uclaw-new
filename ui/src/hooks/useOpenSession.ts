/**
 * useOpenSession — 统一的"打开/聚焦会话 Tab"操作
 *
 * 封装 openTab + setTabs + setActiveTabId + setAppMode + setCurrentXxxId，
 * 确保所有打开会话的入口都能正确同步 appMode 和 currentSessionId。
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { tabsAtom, activeTabIdAtom, openTab, type TabType } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { appModeAtom } from '@/atoms/app-mode'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import {
  currentAgentSessionIdAtom,
  agentSessionsAtom,
  currentAgentWorkspaceIdAtom,
  unviewedCompletedSessionIdsAtom,
} from '@/atoms/agent-atoms'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import {
  automationSelectedSpecIdAtom,
  automationActiveTabAtom,
  automationActivityRunSessionIdAtom,
} from '@/atoms/automation-ui'
import { currentSymphonyWorkflowIdAtom } from '@/atoms/symphony_graph'
type OpenSessionFn = (type: TabType, sessionId: string, title: string) => void

export function useOpenSession(): OpenSessionFn {
  const [tabs, setTabs] = useAtom(tabsAtom)
  const setActiveTabId = useSetAtom(activeTabIdAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setCurrentConversationId = useSetAtom(currentConversationIdAtom)
  const setCurrentAgentSessionId = useSetAtom(currentAgentSessionIdAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const setCurrentAgentWorkspaceId = useSetAtom(currentAgentWorkspaceIdAtom)
  const setUnviewedCompleted = useSetAtom(unviewedCompletedSessionIdsAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setAutomationSelectedSpecId = useSetAtom(automationSelectedSpecIdAtom)
  const setAutomationActiveTab = useSetAtom(automationActiveTabAtom)
  const setAutomationActivityRunSessionId = useSetAtom(automationActivityRunSessionIdAtom)
  const setCurrentSymphonyWorkflowId = useSetAtom(currentSymphonyWorkflowIdAtom)

  return React.useCallback(
    (type: TabType, sessionId: string, title: string): void => {
      // W2: automation sessions route to Kaleidoscope, not workspace.
      // Check any session (type doesn't matter) for automation origin metadata.
      const anySession = agentSessions.find((s) => s.id === sessionId)
      const meta = (() => {
        try { return JSON.parse(anySession?.metadataJson ?? '{}') } catch { return {} }
      })()
      const origin: string = meta.origin ?? ''
      if (origin.startsWith('automation:')) {
        setTopLevelView('kaleidoscope')
        setKaleidoscopeModule('humans')
        setAutomationSelectedSpecId(meta.spec_id ?? null)
        if (origin === 'automation:home_thread') {
          setAutomationActiveTab('chat')
          setAutomationActivityRunSessionId(null)
        } else {
          setAutomationActiveTab('activity')
          setAutomationActivityRunSessionId(sessionId)
        }
        return
      }

      // For agent tabs, prepend the session emoji (if any) to the tab title
      let displayTitle = title
      const session = type === 'agent'
        ? agentSessions.find((s) => s.id === sessionId)
        : undefined
      if (type === 'agent' && session) {
        const emoji = session.titleEmoji
        if (emoji && emoji !== '💬') {
          displayTitle = `${emoji} ${title}`
        }
      }
      // Tab's workspaceId: prefer the session's authoritative workspace
      // (sessions know where they live) over the user's current view —
      // this matters when opening a session from a context that shows
      // cross-workspace results (search, deep-link, recent). Falls back
      // to the active workspace for chat / browser tabs where no
      // per-session workspace exists.
      const ws =
        session?.workspaceId ?? activeWorkspaceId ?? 'default'
      const result = openTab(tabs, { type, sessionId, title: displayTitle, workspaceId: ws })
      setTabs(result.tabs)
      setActiveTabId(result.activeTabId)

      // Only set app mode for chat and agent tabs, not browser
      if (type !== 'browser') {
        setAppMode(type)
      }

      if (type === 'chat') {
        setCurrentConversationId(sessionId)
      } else if (type === 'symphony') {
        // Sentinel sessionId ('__symphony_new__') is reserved for the
        // empty-state hand-off; don't mark it as the "current workflow"
        // since it doesn't exist in the DB.
        if (sessionId !== '__symphony_new__') {
          setCurrentSymphonyWorkflowId(sessionId)
        }
      } else if (type === 'agent') {
        setCurrentAgentSessionId(sessionId)

        // 清除该会话的"已完成未查看"标记，与 TabBar.handleActivate 保持一致
        setUnviewedCompleted((prev) => {
          if (!prev.has(sessionId)) return prev
          const next = new Set(prev)
          next.delete(sessionId)
          return next
        })

        // 同步 workspaceId，确保与 TabBar 切换行为一致
        if (session?.workspaceId) {
          setCurrentAgentWorkspaceId(session.workspaceId)
        }
      }
    },
    [tabs, setTabs, setActiveTabId, setAppMode, setCurrentConversationId, setCurrentAgentSessionId,
     agentSessions, setCurrentAgentWorkspaceId, setUnviewedCompleted, activeWorkspaceId,
     setTopLevelView, setKaleidoscopeModule, setAutomationSelectedSpecId,
     setAutomationActiveTab, setAutomationActivityRunSessionId, setCurrentSymphonyWorkflowId],
  )
}

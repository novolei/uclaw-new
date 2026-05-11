/**
 * TabSessionSyncer — effect-only component that keeps the right
 * panel and chat/agent body content pointed at the active workspace's
 * active tab.
 *
 * Background: per-workspace tab memory (this PR) flips activeTabIdAtom
 * automatically on workspace switch via the per-workspace map. But the
 * body / right-panel are gated on currentAgentSessionIdAtom /
 * currentConversationIdAtom, which are only written by TabBar's click
 * handler. Without this syncer, the body keeps showing the previous
 * workspace's content after a ⌘N switch.
 *
 * The syncer subscribes to activeTabAtom (derived from the per-workspace
 * map) and writes the matching session/conversation atom + appMode
 * whenever the active tab changes.
 *
 * Mounted once in AppShell. Returns null.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { activeTabAtom } from '@/atoms/tab-atoms'
import { appModeAtom } from '@/atoms/app-mode'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'

export function TabSessionSyncer(): null {
  const activeTab = useAtomValue(activeTabAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setCurrentConversationId = useSetAtom(currentConversationIdAtom)
  const setCurrentAgentSessionId = useSetAtom(currentAgentSessionIdAtom)

  React.useEffect(() => {
    if (!activeTab) {
      // Workspace has no active tab — clear the session atoms so the
      // body/right-panel falls back to its empty state.
      setCurrentAgentSessionId(null)
      setCurrentConversationId(null)
      return
    }
    if (activeTab.type === 'agent') {
      setAppMode('agent')
      setCurrentAgentSessionId(activeTab.sessionId)
    } else if (activeTab.type === 'chat') {
      setAppMode('chat')
      setCurrentConversationId(activeTab.sessionId)
    }
    // Browser tabs don't change appMode or session atoms — they keep
    // the prior mode (matches TabBar.handleActivate's behavior).
  }, [activeTab, setAppMode, setCurrentAgentSessionId, setCurrentConversationId])

  return null
}

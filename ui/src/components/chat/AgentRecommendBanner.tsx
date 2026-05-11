/**
 * AgentRecommendBanner — Agent 模式推荐横幅
 *
 * 当 AI 通过 suggest_agent_mode 工具推荐切换到 Agent 模式时，
 * 在 ChatInput 上方展示推荐横幅。
 */

import * as React from 'react'
import { useAtom, useStore } from 'jotai'
import { toast } from 'sonner'
import { Sparkles, X, ArrowRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { pendingAgentRecommendationAtom } from '@/atoms/chat-atoms'
import {
  agentChannelIdAtom,
  agentWorkspacesAtom,
  agentSessionsAtom,
  currentAgentSessionIdAtom,
  currentAgentWorkspaceIdAtom,
  agentPromptSuggestionsAtom,
} from '@/atoms/agent-atoms'
import { activeViewAtom } from '@/atoms/active-view'
import { appModeAtom } from '@/atoms/app-mode'
import { tabsAtom, activeTabIdAtom, openTab } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { createAgentSession, migrateChatToAgent, listAgentSessions, updateSettings } from '@/lib/tauri-bridge'

export function AgentRecommendBanner(): React.ReactElement | null {
  const [recommendation, setRecommendation] = useAtom(pendingAgentRecommendationAtom)
  const store = useStore()
  const [migrating, setMigrating] = React.useState(false)

  if (!recommendation) return null

  const handleDismiss = (): void => {
    setRecommendation(null)
  }

  const handleMigrate = async (): Promise<void> => {
    if (migrating) return

    const agentChannelId = store.get(agentChannelIdAtom)
    if (!agentChannelId) {
      toast.error('请先在设置中配置 Agent 渠道')
      return
    }

    const { conversationId, suggestedPrompt } = recommendation
    setRecommendation(null)

    setMigrating(true)
    try {
      const workspaces = store.get(agentWorkspacesAtom)
      const defaultWorkspaceId = workspaces[0]?.id ?? null

      const session = await createAgentSession(
        undefined,
        agentChannelId,
        defaultWorkspaceId ?? undefined,
      )

      await migrateChatToAgent(conversationId, session.id)

      const sessions = await listAgentSessions()
      store.set(agentSessionsAtom, sessions)

      if (defaultWorkspaceId) {
        store.set(currentAgentWorkspaceIdAtom, defaultWorkspaceId)
        updateSettings({
          agentWorkspaceId: defaultWorkspaceId,
        }).catch(console.error)
      }

      store.set(appModeAtom, 'agent')
      store.set(activeViewAtom, 'conversations')

      const sessionTitle = session.title ?? '新 Agent 会话'
      const tabs = store.get(tabsAtom)
      const ws = store.get(activeWorkspaceIdAtom) ?? 'default'
      const result = openTab(tabs, {
        type: 'agent',
        sessionId: session.id,
        title: sessionTitle,
        workspaceId: ws,
      })
      store.set(tabsAtom, result.tabs)
      store.set(activeTabIdAtom, result.activeTabId)
      store.set(currentAgentSessionIdAtom, session.id)

      store.set(agentPromptSuggestionsAtom, (prev) => {
        const map = new Map(prev)
        map.set(session.id, suggestedPrompt)
        return map
      })

      toast.success('已切换到 Agent 模式', {
        description: '对话历史已迁移到新的 Agent 会话',
      })
    } catch (error) {
      console.error('[AgentRecommendBanner] 迁移失败:', error)
      toast.error('切换到 Agent 模式失败')
    } finally {
      setMigrating(false)
    }
  }

  return (
    <div className="mx-4 mb-3 rounded-xl bg-card shadow-lg overflow-hidden animate-in slide-in-from-bottom-2 duration-200">
      <div className="px-4 pt-3 pb-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="size-4 text-primary" />
            <span className="text-sm font-medium text-foreground">推荐使用 Agent 模式</span>
          </div>
          <button
            type="button"
            className="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
            onClick={handleDismiss}
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      <div className="px-4 pb-3">
        <p className="text-sm text-foreground/80 leading-relaxed">
          {recommendation.reason}
        </p>
      </div>

      <div className="flex items-center justify-end px-4 pb-3">
        <Button
          variant="default"
          size="sm"
          onClick={handleMigrate}
          disabled={migrating}
          className="h-7 px-3 text-xs"
        >
          {migrating ? '切换中...' : '切换到 Agent 模式'}
          {!migrating && <ArrowRight className="size-3 ml-1" />}
        </Button>
      </div>
    </div>
  )
}

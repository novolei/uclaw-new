/**
 * ModeSwitcher — Chat / Agent / Symphony 模式三段切换（带滑动指示器）
 *
 * 切换模式时自动恢复上一次在该模式下查看的对话/会话/工作流：
 * 1. 优先恢复上次选中的 ID
 * 2. 其次查找已打开的同类型 Tab
 * 3. 兜底打开最近的对话/会话/工作流
 * 4. 都没有则仅切换模式（Symphony 还会等待用户从画布创建新工作流）
 */

import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { appModeAtom, type AppMode } from '@/atoms/app-mode'
import { conversationsAtom, currentConversationIdAtom } from '@/atoms/chat-atoms'
import { agentSessionsAtom, currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import {
  currentSymphonyWorkflowIdAtom,
  symphonyWorkflowsAtom,
} from '@/atoms/symphony_graph'
import { visibleTabsAtom } from '@/atoms/tab-atoms'
import { useOpenSession } from '@/hooks/useOpenSession'
import { SYMPHONY_NEW_TAB_SENTINEL } from '@/components/symphony_graph/SymphonyCanvas'
import { Bot, MessageSquare, Network } from 'lucide-react'
import { cn } from '@/lib/utils'

const modes: { value: AppMode; label: string; icon: React.ReactNode }[] = [
  { value: 'agent', label: 'Agent', icon: <Bot size={15} /> },
  { value: 'chat', label: 'Chat', icon: <MessageSquare size={15} /> },
  { value: 'symphony', label: 'Symphony', icon: <Network size={15} /> },
]

const SLIDER_INDEX: Record<AppMode, number> = { agent: 0, chat: 1, symphony: 2 }

export function ModeSwitcher(): React.ReactElement {
  const [mode, setMode] = useAtom(appModeAtom)
  const openSession = useOpenSession()
  const conversations = useAtomValue(conversationsAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const symphonyWorkflows = useAtomValue(symphonyWorkflowsAtom)
  const currentConversationId = useAtomValue(currentConversationIdAtom)
  const currentAgentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const currentSymphonyWorkflowId = useAtomValue(currentSymphonyWorkflowIdAtom)
  const tabs = useAtomValue(visibleTabsAtom)

  /** 尝试恢复目标模式下的上一个对话/会话/工作流，按优先级 fallback */
  const restoreSession = React.useCallback(
    (targetMode: AppMode) => {
      // 1. 上次选中的 id 仍存在 → 恢复
      if (targetMode === 'chat') {
        if (currentConversationId) {
          const match = conversations.find((s) => s.id === currentConversationId)
          if (match) {
            openSession('chat', match.id, match.title)
            return
          }
        }
      } else if (targetMode === 'agent') {
        if (currentAgentSessionId) {
          const match = agentSessions.find((s) => s.id === currentAgentSessionId)
          if (match) {
            openSession('agent', match.id, match.title)
            return
          }
        }
      } else {
        // symphony
        if (currentSymphonyWorkflowId) {
          const match = symphonyWorkflows.find(
            (w) => w.id === currentSymphonyWorkflowId,
          )
          if (match) {
            openSession('symphony', match.id, match.name)
            return
          }
        }
      }
      // 2. 已打开的同类型 Tab → 聚焦
      const tab = tabs.find((t) => t.type === targetMode)
      if (tab) {
        openSession(targetMode, tab.sessionId, tab.title)
        return
      }
      // 3. 最近的未归档项目 → 打开
      if (targetMode === 'chat') {
        const recent = conversations.find((s) => !s.archived)
        if (recent) {
          openSession('chat', recent.id, recent.title)
          return
        }
      } else if (targetMode === 'agent') {
        const recent = agentSessions.find((s) => !s.archived)
        if (recent) {
          openSession('agent', recent.id, recent.title)
          return
        }
      } else if (targetMode === 'symphony') {
        const recent = symphonyWorkflows[0]
        if (recent) {
          openSession('symphony', recent.id, recent.name)
          return
        }
        // 零 workflow：开 sentinel tab，SymphonyCanvas 检测到后渲染
        // 空状态 + 模板画廊，让用户一键创建第一个 workflow。
        openSession('symphony', SYMPHONY_NEW_TAB_SENTINEL, 'New workflow')
        return
      }
      // 4. 没有任何项目，仅切换模式
      setMode(targetMode)
    },
    [
      openSession,
      conversations,
      agentSessions,
      symphonyWorkflows,
      currentConversationId,
      currentAgentSessionId,
      currentSymphonyWorkflowId,
      tabs,
      setMode,
    ],
  )

  const handleModeSwitch = React.useCallback(
    (targetMode: AppMode) => {
      if (targetMode === mode) return
      restoreSession(targetMode)
    },
    [mode, restoreSession],
  )

  return (
    <div className="pt-2">
      <div className="relative flex rounded-xl bg-muted p-1">
        {/* 滑动背景指示器 — 三段宽度: each 33.333% minus 4px slider gap */}
        <div
          className={cn(
            'mode-slider absolute top-1 bottom-1 rounded-lg bg-background shadow-sm transition-transform duration-300 ease-in-out',
          )}
          style={{
            width: 'calc(33.333% - 4px)',
            transform: `translateX(calc(${SLIDER_INDEX[mode] * 100}% + ${SLIDER_INDEX[mode] * 4}px))`,
          }}
        />
        {modes.map(({ value, label, icon }) => (
          <button
            key={value}
            onClick={() => handleModeSwitch(value)}
            className={cn(
              'mode-btn relative z-[1] flex-1 flex items-center justify-center gap-1.5 rounded-lg px-3 py-1 text-sm font-medium transition-colors duration-200',
              mode === value
                ? 'mode-btn-selected text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {icon}
            {label}
          </button>
        ))}
      </div>
    </div>
  )
}

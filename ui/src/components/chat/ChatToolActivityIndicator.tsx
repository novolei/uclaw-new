/**
 * ChatToolActivityIndicator - Chat 模式工具活动指示器
 *
 * 把 ChatToolActivity[] 的 start/result 事件按 toolCallId 合并后，
 * 用 ChatToolBlock 纵向排布每个工具调用。
 */

import * as React from 'react'
import { ChatToolBlock } from './ChatToolBlock'
import type { ChatToolActivity } from '@/lib/chat-types'

interface MergedActivity {
  toolName: string
  done: boolean
  isError?: boolean
  result?: string
  input: Record<string, unknown>
}

export function ChatToolActivityIndicator({
  activities,
  isStreaming = false,
}: {
  activities: ChatToolActivity[]
  isStreaming?: boolean
}): React.ReactElement | null {
  const merged = React.useMemo(() => {
    const map = new Map<string, MergedActivity>()
    for (const a of activities) {
      const existing = map.get(a.toolCallId)
      if (a.type === 'start') {
        map.set(a.toolCallId, {
          toolName: a.toolName,
          done: false,
          input: a.input ?? existing?.input ?? {},
        })
      } else if (a.type === 'result') {
        map.set(a.toolCallId, {
          toolName: existing?.toolName ?? a.toolName,
          done: true,
          isError: a.isError,
          result: a.result,
          input: a.input ?? existing?.input ?? {},
        })
      }
    }
    return Array.from(map.entries())
  }, [activities])

  if (merged.length === 0) return null

  return (
    <div className="mb-2 space-y-px">
      {merged.map(([callId, item], idx) => (
        <ChatToolBlock
          key={callId}
          toolName={item.toolName}
          input={item.input}
          result={item.result}
          isError={item.isError}
          isCompleted={item.done}
          animate={isStreaming}
          index={idx}
        />
      ))}
    </div>
  )
}

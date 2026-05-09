/**
 * ChatToolActivityIndicator - Chat 模式工具活动指示器
 *
 * 将 ChatToolActivity[] 的 start/result 事件合并后，
 * 沿一根垂直时间线纵向排布每个 ChatToolBlock，制造时间流动感。
 */

import * as React from 'react'
import { ChatToolBlock } from './ChatToolBlock'
import type { ChatToolActivity } from '@/lib/proma-types'

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
    <div className="relative mb-2 pl-1">
      {/* 时间线主干：从第一个 dot 中心延伸到最后一个 dot 中心 */}
      <span
        aria-hidden="true"
        className="absolute left-[11px] top-[14px] bottom-[14px] w-px bg-gradient-to-b from-border/30 via-border/60 to-border/30 dark:from-border/40 dark:via-border/70 dark:to-border/40"
      />
      <div className="space-y-px">
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
    </div>
  )
}

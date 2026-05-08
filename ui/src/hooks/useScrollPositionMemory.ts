/**
 * ScrollPositionManager — 切换会话时把视图重置到底部
 *
 * 必须放在 <Conversation> 内部使用（依赖 useConversationContext）。
 * 当 `id` 变化或首次 ready 时，自动滚到底部，让用户进入任意会话默认看到最新消息。
 */

import * as React from 'react'
import { useConversationContext } from '@/components/ai-elements/conversation'

interface ScrollPositionManagerProps {
  /** 会话/Session ID — 变化时触发重置 */
  id: string
  /** 数据是否已加载就绪，false 时不重置（避免在空内容时滚动无效） */
  ready: boolean
}

export function ScrollPositionManager({ id, ready }: ScrollPositionManagerProps): React.ReactElement | null {
  const ctx = useConversationContext()
  const lastIdRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (!ready || !ctx) return
    // 同 id 已处理过则跳过；id 变化或首次 ready 时滚到底
    if (lastIdRef.current === id) return
    lastIdRef.current = id

    // 等下一帧让消息列表先 paint，再滚动到底（否则 scrollHeight 还没更新到位）
    const raf = window.requestAnimationFrame(() => {
      ctx.scrollToBottom('auto')
    })
    return () => window.cancelAnimationFrame(raf)
  }, [id, ready, ctx])

  return null
}

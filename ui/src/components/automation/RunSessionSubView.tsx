import { useEffect, useState } from 'react'
import { getAgentSessionMessages } from '@/lib/tauri-bridge'
import { AgentMessages } from '@/components/agent/AgentMessages'
import type { AgentMessage } from '@/lib/agent-types'

interface Props {
  sessionId: string
  /** True while the activity record is still running or queued. Enables
   *  message polling so the transcript stays live during execution. */
  isRunning?: boolean
  onBack: () => void
}

export function RunSessionSubView({ sessionId, isRunning, onBack }: Props) {
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)

  // Initial load
  useEffect(() => {
    setLoaded(false)
    getAgentSessionMessages(sessionId).then((msgs) => {
      setMessages(msgs as AgentMessage[])
      setLoaded(true)
    })
  }, [sessionId])

  // Poll while the run is still active so the transcript stays live.
  useEffect(() => {
    if (!isRunning) return
    const id = setInterval(() => {
      getAgentSessionMessages(sessionId).then((msgs) =>
        setMessages(msgs as AgentMessage[])
      )
    }, 2000)
    return () => clearInterval(id)
  }, [isRunning, sessionId])

  return (
    <div className="flex flex-col h-full">
      {/* breadcrumb */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-border/50 text-xs text-muted-foreground shrink-0">
        <button
          onClick={onBack}
          className="titlebar-no-drag text-primary hover:underline"
        >
          ← 动态
        </button>
        <span>/</span>
        <span>运行详情</span>
        {isRunning && (
          <span className="ml-auto flex items-center gap-1 text-primary">
            <span className="size-1.5 rounded-full bg-primary animate-pulse" />
            运行中
          </span>
        )}
      </div>

      {/* transcript */}
      <div className="flex-1 overflow-hidden">
        <AgentMessages
          sessionId={sessionId}
          messages={messages}
          messagesLoaded={loaded}
          streaming={false}
        />
      </div>
    </div>
  )
}

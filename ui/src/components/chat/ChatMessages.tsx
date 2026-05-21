/**
 * ChatMessages - 消息区域
 *
 * 使用 Conversation / ConversationContent / ConversationScrollButton 原语
 * 替代手动 scroll。支持上下文分隔线和并排模式切换。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Loader2 } from 'lucide-react'
import { WelcomeEmptyState } from '@/components/welcome/WelcomeEmptyState'
import { ChatMessageItem, formatMessageTime } from './ChatMessageItem'
import type { InlineEditSubmitPayload } from './ChatMessageItem'
import { ChatToolActivityIndicator } from './ChatToolActivityIndicator'
import { ParallelChatMessages } from './ParallelChatMessages'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageLoading,
  MessageResponse,
  StreamingIndicator,
} from '@/components/ai-elements/message'
import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from '@/components/ai-elements/conversation'
import { ScrollMinimap } from '@/components/ai-elements/scroll-minimap'
import type { MinimapItem } from '@/components/ai-elements/scroll-minimap'
import { useConversationContext } from '@/components/ai-elements/conversation'
import { ContextDivider } from '@/components/ai-elements/context-divider'
import {
  Reasoning,
  ReasoningTrigger,
  ReasoningContent,
} from '@/components/ai-elements/reasoning'
import { useSmoothStream } from '@/hooks/useSmoothStream'
import { ScrollPositionManager } from '@/hooks/useScrollPositionMemory'
import { useConversationParallelMode } from '@/hooks/useConversationSettings'
import { getModelLogo } from '@/lib/model-logo'
import { userProfileAtom } from '@/atoms/user-profile'
import { tabMinimapCacheAtom, type TabMinimapItem } from '@/atoms/tab-atoms'
import { agentDisplayNameForAtom } from '@/atoms/agent-display-name'
import type { ChatMessage, ChatToolActivity } from '@/lib/chat-types'

// ===== 滚动到顶部加载更多 =====

interface ScrollTopLoaderProps {
  hasMore: boolean
  loading: boolean
  onLoadMore: () => Promise<void>
}

function ScrollTopLoader({ hasMore, loading, onLoadMore }: ScrollTopLoaderProps): React.ReactElement | null {
  const ctx = useConversationContext()
  const scrollRef = ctx?.scrollRef ?? { current: null }
  const triggeredRef = React.useRef(false)

  React.useEffect(() => {
    triggeredRef.current = false
  }, [hasMore])

  React.useEffect(() => {
    const el = scrollRef.current
    if (!el || !hasMore || triggeredRef.current) return

    const handleScroll = (): void => {
      if (el.scrollTop < 100 && !triggeredRef.current) {
        triggeredRef.current = true
        const prevHeight = el.scrollHeight

        onLoadMore().then(() => {
          requestAnimationFrame(() => {
            el.scrollTop = el.scrollHeight - prevHeight
          })
        })
      }
    }

    el.addEventListener('scroll', handleScroll, { passive: true })
    return () => el.removeEventListener('scroll', handleScroll)
  }, [scrollRef, hasMore, onLoadMore])

  if (!hasMore) return null

  if (loading) {
    return (
      <div className="flex justify-center py-3">
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return null
}

// ===== 主组件 =====

interface ChatMessagesProps {
  conversationId: string
  messages: ChatMessage[]
  messagesLoaded: boolean
  streaming: boolean
  streamingContent: string
  streamingReasoning: string
  streamingModel: string | null
  startedAt?: number
  toolActivities: ChatToolActivity[]
  contextDividers: string[]
  hasMore: boolean
  onDeleteMessage?: (messageId: string) => Promise<void>
  onResendMessage?: (message: ChatMessage) => Promise<void>
  onStartInlineEdit?: (message: ChatMessage) => void
  onSubmitInlineEdit?: (message: ChatMessage, payload: InlineEditSubmitPayload) => Promise<void>
  onCancelInlineEdit?: () => void
  inlineEditingMessageId?: string | null
  onDeleteDivider?: (messageId: string) => void
  onLoadMore?: () => Promise<void>
}

function EmptyState(): React.ReactElement {
  return <WelcomeEmptyState />
}

export function ChatMessages({
  conversationId,
  messages,
  messagesLoaded,
  streaming,
  streamingContent,
  streamingReasoning,
  streamingModel,
  startedAt,
  toolActivities,
  contextDividers,
  hasMore,
  onDeleteMessage,
  onResendMessage,
  onStartInlineEdit,
  onSubmitInlineEdit,
  onCancelInlineEdit,
  inlineEditingMessageId,
  onDeleteDivider,
  onLoadMore,
}: ChatMessagesProps): React.ReactElement {
  const userProfile = useAtomValue(userProfileAtom)
  const setMinimapCache = useSetAtom(tabMinimapCacheAtom)
  const agentNameLookup = useAtomValue(agentDisplayNameForAtom)

  // 平滑流式输出
  const { displayedContent: rawSmoothContent } = useSmoothStream({
    content: streamingContent,
    isStreaming: streaming,
  })
  const { displayedContent: rawSmoothReasoning } = useSmoothStream({
    content: streamingReasoning,
    isStreaming: streaming,
  })

  const smoothContent = (streaming || streamingContent) ? rawSmoothContent : ''
  const smoothReasoning = (streaming || streamingReasoning) ? rawSmoothReasoning : ''
  const [parallelMode] = useConversationParallelMode()

  const [loadingMore, setLoadingMore] = React.useState(false)

  const [transitioningCooldown, setTransitioningCooldown] = React.useState(false)
  const wasStreamingRef = React.useRef(streaming)

  const needsInstant = !streaming && (!!streamingContent || !!smoothContent)

  React.useEffect(() => {
    if (wasStreamingRef.current && !streaming) {
      setTransitioningCooldown(true)
    }
    wasStreamingRef.current = streaming
  }, [streaming])

  React.useEffect(() => {
    if (needsInstant) return
    const timer = setTimeout(() => setTransitioningCooldown(false), 150)
    return () => clearTimeout(timer)
  }, [needsInstant])

  const transitioning = needsInstant || transitioningCooldown

  const [ready, setReady] = React.useState(false)
  const prevConversationIdRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (conversationId !== prevConversationIdRef.current) {
      prevConversationIdRef.current = conversationId
      setReady(false)
    }
  }, [conversationId])

  React.useEffect(() => {
    if (ready) return
    if (!messagesLoaded) return

    if (messages.length === 0 && !streaming) {
      setReady(true)
      return
    }

    let cancelled = false
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!cancelled) setReady(true)
      })
    })
    return () => { cancelled = true }
  }, [messages, streaming, ready, messagesLoaded])

  const handleLoadMore = React.useCallback(async () => {
    if (!onLoadMore || loadingMore || !hasMore) return

    setLoadingMore(true)
    await onLoadMore()
    setLoadingMore(false)
  }, [onLoadMore, loadingMore, hasMore])

  React.useEffect(() => {
    if (parallelMode && hasMore) {
      handleLoadMore()
    }
  }, [parallelMode, hasMore, handleLoadMore])

  const minimapItems: MinimapItem[] = React.useMemo(
    () => messages.map((m) => ({
      id: m.id,
      role: m.role as MinimapItem['role'],
      preview: m.content.slice(0, 200),
      avatar: m.role === 'user' ? userProfile.avatar : undefined,
      model: m.model,
    })),
    [messages, userProfile.avatar]
  )

  React.useEffect(() => {
    if (minimapItems.length > 0) {
      setMinimapCache((prev) => {
        const next = new Map(prev)
        next.set(conversationId, minimapItems as unknown as TabMinimapItem[])
        return next
      })
    }
  }, [conversationId, minimapItems, setMinimapCache])

  if (parallelMode) {
    return (
      <ParallelChatMessages
        messages={messages}
        conversationId={conversationId}
        streaming={streaming}
        streamingContent={smoothContent}
        streamingReasoning={smoothReasoning}
        startedAt={startedAt}
        contextDividers={contextDividers}
        onDeleteDivider={onDeleteDivider}
        onDeleteMessage={onDeleteMessage}
        onResendMessage={onResendMessage}
        onStartInlineEdit={onStartInlineEdit}
        onSubmitInlineEdit={onSubmitInlineEdit}
        onCancelInlineEdit={onCancelInlineEdit}
        inlineEditingMessageId={inlineEditingMessageId}
        loadingMore={loadingMore}
      />
    )
  }

  const dividerSet = new Set(contextDividers)

  return (
    <Conversation resize={ready && !transitioning ? 'smooth' : 'instant'} className={ready ? 'opacity-100 transition-opacity duration-200' : 'opacity-0'}>
      <ScrollPositionManager id={conversationId} ready={ready} />
      <ScrollTopLoader
        hasMore={hasMore}
        loading={loadingMore}
        onLoadMore={handleLoadMore}
      />
      <ConversationContent>
        {messages.length === 0 && !streaming ? (
          <EmptyState />
        ) : (
          <>
            {messages.map((msg: ChatMessage) => (
              <React.Fragment key={msg.id}>
                <div data-message-id={msg.id}>
                  <ChatMessageItem
                    message={msg}
                    conversationId={conversationId}
                    isStreaming={false}
                    isLastAssistant={false}
                    allMessages={messages}
                    onDeleteMessage={onDeleteMessage}
                    onResendMessage={onResendMessage}
                    onStartInlineEdit={onStartInlineEdit}
                    onSubmitInlineEdit={onSubmitInlineEdit}
                    onCancelInlineEdit={onCancelInlineEdit}
                    isInlineEditing={msg.id === inlineEditingMessageId}
                  />
                </div>
                {dividerSet.has(msg.id) && (
                  <ContextDivider
                    messageId={msg.id}
                    onDelete={onDeleteDivider}
                  />
                )}
              </React.Fragment>
            ))}

            {(streaming || smoothContent || smoothReasoning) && (
              <Message from="assistant">
                <MessageHeader
                  name={agentNameLookup(conversationId)}
                  model={streamingModel ?? undefined}
                  time={formatMessageTime(Date.now())}
                  logo={
                    <img
                      src={getModelLogo(streamingModel ?? '')}
                      alt="AI"
                      className="size-[35px] rounded-[25%] object-cover"
                    />
                  }
                />
                <MessageContent>
                  <ChatToolActivityIndicator activities={toolActivities} isStreaming={streaming} />

                  {smoothReasoning && (
                    <Reasoning
                      isStreaming={streaming && !smoothContent}
                      defaultOpen={true}
                    >
                      <ReasoningTrigger />
                      <ReasoningContent>{smoothReasoning}</ReasoningContent>
                    </Reasoning>
                  )}

                  {smoothContent ? (
                    <>
                      <MessageResponse>{smoothContent}</MessageResponse>
                      {streaming && <StreamingIndicator />}
                    </>
                  ) : (
                    streaming && !smoothReasoning && <MessageLoading startedAt={startedAt} />
                  )}
                </MessageContent>
                <div className="pl-[46px] mt-0.5 min-h-[28px]" />
              </Message>
            )}
          </>
        )}
      </ConversationContent>
      <ScrollMinimap items={minimapItems} />
      <ConversationScrollButton />
    </Conversation>
  )
}

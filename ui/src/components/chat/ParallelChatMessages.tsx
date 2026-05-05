/**
 * ParallelChatMessages - 并排消息展示
 *
 * 两列布局：用户消息 | 助手回复
 * - 按上下文分隔线分段
 * - 各列独立滚动
 * - ChatMessageItem 以 isParallelMode={true} 渲染
 */

import { Fragment, useMemo, useRef, useEffect } from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { ChatMessageItem, formatMessageTime } from './ChatMessageItem'
import type { InlineEditSubmitPayload } from './ChatMessageItem'
import { ContextDivider } from '@/components/ai-elements/context-divider'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageLoading,
  MessageResponse,
  StreamingIndicator,
} from '@/components/ai-elements/message'
import {
  Reasoning,
  ReasoningTrigger,
  ReasoningContent,
} from '@/components/ai-elements/reasoning'
import { streamingModelAtom } from '@/atoms/chat-atoms'
import { getModelLogo } from '@/lib/model-logo'
import type { ChatMessage } from '@/lib/proma-types'

interface MessageSegment {
  userMessages: ChatMessage[]
  assistantMessages: ChatMessage[]
  dividerMessageId?: string
}

interface ParallelChatMessagesProps {
  messages: ChatMessage[]
  conversationId?: string
  streaming: boolean
  streamingContent: string
  streamingReasoning: string
  startedAt?: number
  contextDividers?: string[]
  onDeleteDivider?: (messageId: string) => void
  onDeleteMessage?: (messageId: string) => Promise<void>
  onResendMessage?: (message: ChatMessage) => Promise<void>
  onStartInlineEdit?: (message: ChatMessage) => void
  onSubmitInlineEdit?: (message: ChatMessage, payload: InlineEditSubmitPayload) => Promise<void>
  onCancelInlineEdit?: () => void
  inlineEditingMessageId?: string | null
  loadingMore?: boolean
}

function EmptyColumn({ side }: { side: 'user' | 'assistant' }): React.ReactElement {
  return (
    <div className="flex h-full items-center justify-center p-4">
      <p className="text-sm text-muted-foreground">
        {side === 'user' ? '暂无用户消息' : '暂无助手回复'}
      </p>
    </div>
  )
}

function LoadMoreSpinner(): React.ReactElement {
  return (
    <div className="flex items-center justify-center py-3">
      <Loader2 className="size-4 animate-spin text-muted-foreground" />
    </div>
  )
}

function segmentMessages(
  messages: ChatMessage[],
  contextDividers: string[]
): MessageSegment[] {
  const dividerSet = new Set(contextDividers)
  const segments: MessageSegment[] = []
  let currentUserMessages: ChatMessage[] = []
  let currentAssistantMessages: ChatMessage[] = []

  for (const message of messages) {
    if (message.role === 'user') {
      currentUserMessages.push(message)
    } else if (message.role === 'assistant') {
      currentAssistantMessages.push(message)
    }

    if (dividerSet.has(message.id)) {
      segments.push({
        userMessages: currentUserMessages,
        assistantMessages: currentAssistantMessages,
        dividerMessageId: message.id,
      })
      currentUserMessages = []
      currentAssistantMessages = []
    }
  }

  if (currentUserMessages.length > 0 || currentAssistantMessages.length > 0) {
    segments.push({
      userMessages: currentUserMessages,
      assistantMessages: currentAssistantMessages,
    })
  }

  return segments
}

interface MessageColumnProps {
  messages: ChatMessage[]
  allMessages: ChatMessage[]
  conversationId?: string
  onDeleteMessage?: (messageId: string) => Promise<void>
  onResendMessage?: (message: ChatMessage) => Promise<void>
  onStartInlineEdit?: (message: ChatMessage) => void
  onSubmitInlineEdit?: (message: ChatMessage, payload: InlineEditSubmitPayload) => Promise<void>
  onCancelInlineEdit?: () => void
  inlineEditingMessageId?: string | null
  side: 'user' | 'assistant'
  streaming?: boolean
  streamingContent?: string
  streamingReasoning?: string
  startedAt?: number
}

function MessageColumn({
  messages,
  allMessages,
  conversationId,
  onDeleteMessage,
  onResendMessage,
  onStartInlineEdit,
  onSubmitInlineEdit,
  onCancelInlineEdit,
  inlineEditingMessageId,
  side,
  streaming = false,
  streamingContent = '',
  streamingReasoning = '',
  startedAt,
}: MessageColumnProps): React.ReactElement {
  const streamingModel = useAtomValue(streamingModelAtom)
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (scrollRef.current && messages.length > 0) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [messages])

  useEffect(() => {
    if (side === 'assistant' && streaming && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [streaming, streamingContent, streamingReasoning, side])

  if (messages.length === 0 && !(side === 'assistant' && streaming)) {
    return <EmptyColumn side={side} />
  }

  return (
    <div
      ref={scrollRef}
      className="flex-1 min-h-0 overflow-y-auto scrollbar-none overscroll-contain"
    >
      <div className="flex flex-col gap-6 p-4">
        {messages.map((message) => (
          <ChatMessageItem
            key={message.id}
            message={message}
            conversationId={conversationId}
            allMessages={allMessages}
            onDeleteMessage={onDeleteMessage}
            onResendMessage={onResendMessage}
            onStartInlineEdit={onStartInlineEdit}
            onSubmitInlineEdit={onSubmitInlineEdit}
            onCancelInlineEdit={onCancelInlineEdit}
            isInlineEditing={message.id === inlineEditingMessageId}
            isParallelMode
          />
        ))}
        {side === 'assistant' && (streaming || streamingContent || streamingReasoning) && (
          <Message from="assistant">
            <MessageHeader
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
              {streamingReasoning && (
                <Reasoning isStreaming={streaming && !streamingContent} defaultOpen={true}>
                  <ReasoningTrigger />
                  <ReasoningContent>{streamingReasoning}</ReasoningContent>
                </Reasoning>
              )}
              {streamingContent ? (
                <>
                  <MessageResponse>{streamingContent}</MessageResponse>
                  {streaming && <StreamingIndicator />}
                </>
              ) : (
                streaming && !streamingReasoning && <MessageLoading startedAt={startedAt} />
              )}
            </MessageContent>
          </Message>
        )}
      </div>
    </div>
  )
}

export function ParallelChatMessages({
  messages,
  conversationId,
  streaming,
  streamingContent,
  streamingReasoning,
  startedAt,
  contextDividers = [],
  onDeleteDivider,
  onDeleteMessage,
  onResendMessage,
  onStartInlineEdit,
  onSubmitInlineEdit,
  onCancelInlineEdit,
  inlineEditingMessageId,
  loadingMore = false,
}: ParallelChatMessagesProps): React.ReactElement {
  const segments = useMemo(
    () => segmentMessages(messages, contextDividers),
    [messages, contextDividers]
  )

  const userMessages = useMemo(
    () => messages.filter((m) => m.role === 'user'),
    [messages]
  )
  const assistantMessages = useMemo(
    () => messages.filter((m) => m.role === 'assistant'),
    [messages]
  )

  if (segments.length <= 1) {
    return (
      <div className="relative flex-1 min-h-0">
        {loadingMore && (
          <div className="absolute top-0 left-0 right-0 z-10">
            <LoadMoreSpinner />
          </div>
        )}

        <div className="absolute inset-0 flex">
          <div className="w-1/2 flex flex-col overflow-hidden border-r border-border">
            <div className="px-4 py-2 border-b border-border bg-muted/30">
              <span className="text-sm font-medium text-muted-foreground">
                用户消息
              </span>
            </div>
            <MessageColumn
              messages={userMessages}
              allMessages={messages}
              conversationId={conversationId}
              onDeleteMessage={onDeleteMessage}
              onResendMessage={onResendMessage}
              onStartInlineEdit={onStartInlineEdit}
              onSubmitInlineEdit={onSubmitInlineEdit}
              onCancelInlineEdit={onCancelInlineEdit}
              inlineEditingMessageId={inlineEditingMessageId}
              side="user"
            />
          </div>

          <div className="w-1/2 flex flex-col overflow-hidden">
            <div className="px-4 py-2 border-b border-border bg-muted/30">
              <span className="text-sm font-medium text-muted-foreground">
                助手回复
              </span>
            </div>
            <MessageColumn
              messages={assistantMessages}
              allMessages={messages}
              conversationId={conversationId}
              onDeleteMessage={onDeleteMessage}
              onResendMessage={onResendMessage}
              onStartInlineEdit={onStartInlineEdit}
              onSubmitInlineEdit={onSubmitInlineEdit}
              onCancelInlineEdit={onCancelInlineEdit}
              inlineEditingMessageId={inlineEditingMessageId}
              side="assistant"
              streaming={streaming}
              streamingContent={streamingContent}
              streamingReasoning={streamingReasoning}
              startedAt={startedAt}
            />
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="relative flex-1 min-h-0">
      <div className="absolute inset-0 flex flex-col overflow-hidden">
        {loadingMore && <LoadMoreSpinner />}

        {segments.map((segment, index) => (
          <Fragment key={index}>
            <div
              className={
                index === segments.length - 1
                  ? 'flex flex-1 min-h-0 overflow-hidden'
                  : 'flex flex-shrink-0 overflow-hidden'
              }
            >
              <div className="w-1/2 flex flex-col overflow-hidden border-r border-border">
                {index === 0 && (
                  <div className="px-4 py-2 border-b border-border bg-muted/30">
                    <span className="text-sm font-medium text-muted-foreground">
                      用户消息
                    </span>
                  </div>
                )}
                <MessageColumn
                  messages={segment.userMessages}
                  allMessages={messages}
                  conversationId={conversationId}
                  onDeleteMessage={onDeleteMessage}
                  onResendMessage={onResendMessage}
                  onStartInlineEdit={onStartInlineEdit}
                  onSubmitInlineEdit={onSubmitInlineEdit}
                  onCancelInlineEdit={onCancelInlineEdit}
                  inlineEditingMessageId={inlineEditingMessageId}
                  side="user"
                />
              </div>

              <div className="w-1/2 flex flex-col overflow-hidden">
                {index === 0 && (
                  <div className="px-4 py-2 border-b border-border bg-muted/30">
                    <span className="text-sm font-medium text-muted-foreground">
                      助手回复
                    </span>
                  </div>
                )}
                <MessageColumn
                  messages={segment.assistantMessages}
                  allMessages={messages}
                  conversationId={conversationId}
                  onDeleteMessage={onDeleteMessage}
                  onResendMessage={onResendMessage}
                  onStartInlineEdit={onStartInlineEdit}
                  onSubmitInlineEdit={onSubmitInlineEdit}
                  onCancelInlineEdit={onCancelInlineEdit}
                  inlineEditingMessageId={inlineEditingMessageId}
                  side="assistant"
                  streaming={index === segments.length - 1 ? streaming : false}
                  streamingContent={index === segments.length - 1 ? streamingContent : ''}
                  streamingReasoning={index === segments.length - 1 ? streamingReasoning : ''}
                  startedAt={index === segments.length - 1 ? startedAt : undefined}
                />
              </div>
            </div>

            {segment.dividerMessageId && (
              <ContextDivider
                messageId={segment.dividerMessageId}
                onDelete={onDeleteDivider}
                className="flex-shrink-0"
              />
            )}
          </Fragment>
        ))}
      </div>
    </div>
  )
}

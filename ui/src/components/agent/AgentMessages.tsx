/**
 * AgentMessages — Agent 消息列表
 *
 * 复用 Chat 的 Conversation/Message 原语组件渲染持久化消息与流式气泡。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Bot, FileText, FileImage, RotateCw, AlertTriangle, ChevronDown, ChevronRight, Download, Zap } from 'lucide-react'
import { ImageLightbox } from '@/components/ui/image-lightbox'
import { WelcomeEmptyState } from '@/components/welcome/WelcomeEmptyState'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageActions,
  MessageResponse,
  UserMessageContent,
  BasePathsProvider,
} from '@/components/ai-elements/message'
import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from '@/components/ai-elements/conversation'
import { ScrollMinimap } from '@/components/ai-elements/scroll-minimap'
import type { MinimapItem } from '@/components/ai-elements/scroll-minimap'
import { StickyUserMessage } from '@/components/ai-elements/sticky-user-message'
import { useSmoothStream } from '@/lib/proma-ui'
import { UserAvatar } from '@/components/chat/UserAvatar'
import { CopyButton } from '@/components/chat/CopyButton'
import { formatMessageTime } from '@/components/chat/ChatMessageItem'
import { Button } from '@/components/ui/button'
import { getModelLogo, resolveModelDisplayName } from '@/lib/model-logo'
import { ToolActivityList } from './ToolActivityItem'
import { ThinkingBlock } from './ContentBlock'
import { NativeBlockRenderer } from './NativeBlockRenderer'
import { ChatToolActivityIndicator } from '@/components/chat/ChatToolActivityIndicator'
import { userProfileAtom } from '@/atoms/user-profile'
import { tabMinimapCacheAtom } from '@/atoms/tab-atoms'
import { channelsAtom } from '@/atoms/chat-atoms'
import { proactiveLearningEventsAtom } from '@/atoms/agent-atoms'
import { ProactiveLearningChip } from '@/components/chat/ProactiveLearningChip'
import { parseSkillCitations } from '@/lib/skill-citation'
import { SkillCitationChips } from './SkillCitationChips'
import { ScrollPositionManager } from '@/hooks/useScrollPositionMemory'
import { cn } from '@/lib/utils'
import { Spinner } from '@/components/ui/spinner'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { CompactingIndicator } from './SDKMessageRenderer'
import type { AgentMessage, AgentEventUsage, RetryAttempt } from '@/lib/agent-types'
import type { ToolActivity, AgentStreamState } from '@/atoms/agent-atoms'
import { readAttachment, saveImageAs } from '@/lib/tauri-bridge'

/** AgentMessages 属性接口 */
interface AgentMessagesProps {
  sessionId: string
  /** 用户在前端选择的模型 ID（用于显示渠道配置的 Model Name） */
  sessionModelId?: string
  messages: AgentMessage[]
  /** 消息是否已完成首次加载 */
  messagesLoaded?: boolean
  streaming: boolean
  streamState?: AgentStreamState
  /** 实时消息列表（流式期间累积，用于过渡状态判断） */
  liveMessages?: any[]
  /** 当前会话工作目录，用于解析相对文件路径 */
  sessionPath?: string | null
  /** 附加目录列表（与 sessionPath 一并用作相对路径解析候选） */
  attachedDirs?: string[]
  /** 最后一轮是否被用户中断 */
  stoppedByUser?: boolean
  onRetry?: () => void
  onRetryInNewSession?: () => void
  onFork?: (upToMessageUuid: string) => void
  onRewind?: (assistantMessageUuid: string) => void
  onCompact?: () => void
}

/** 空状态引导 — 使用 WelcomeEmptyState */
function EmptyState(): React.ReactElement {
  return <WelcomeEmptyState />
}

function AssistantLogo({ model }: { model?: string }): React.ReactElement {
  if (model) {
    return (
      <img
        src={getModelLogo(model)}
        alt={model}
        className="size-[35px] rounded-[25%] object-cover"
      />
    )
  }
  return (
    <div className="size-[35px] rounded-[25%] bg-primary/10 flex items-center justify-center">
      <Bot size={18} className="text-primary" />
    </div>
  )
}

/** 单张工具结果图片（内联显示），点击可预览大图 */
function InlineImage({ attachment }: { attachment: { localPath: string; filename: string; mediaType: string } }): React.ReactElement {
  const [imageSrc, setImageSrc] = React.useState<string | null>(null)
  const [lightboxOpen, setLightboxOpen] = React.useState(false)

  React.useEffect(() => {
    readAttachment(attachment.localPath)
      .then((base64: string) => {
        setImageSrc(`data:${attachment.mediaType};base64,${base64}`)
      })
      .catch((error: unknown) => {
        console.error('[InlineImage] 读取附件失败:', error)
      })
  }, [attachment.localPath, attachment.mediaType])

  const handleSave = React.useCallback((): void => {
    saveImageAs(attachment.localPath, attachment.filename)
  }, [attachment.localPath, attachment.filename])

  if (!imageSrc) {
    return <div className="size-[280px] rounded-lg bg-muted/30 animate-pulse shrink-0" />
  }

  return (
    <div className="relative group inline-block">
      <img
        src={imageSrc}
        alt={attachment.filename}
        className="size-[280px] rounded-lg object-cover shrink-0 cursor-pointer"
        onClick={() => setLightboxOpen(true)}
      />
      <button
        type="button"
        onClick={handleSave}
        className="absolute bottom-2 right-2 p-1.5 rounded-md bg-black/50 text-white opacity-0 group-hover:opacity-100 transition-opacity hover:bg-black/70"
        title="保存图片"
      >
        <Download className="size-4" />
      </button>
      <ImageLightbox
        src={imageSrc}
        alt={attachment.filename}
        open={lightboxOpen}
        onOpenChange={setLightboxOpen}
      />
    </div>
  )
}

/** 从工具活动中提取并内联显示所有生成的图片 */
function ToolResultInlineImages({ activities }: { activities: ToolActivity[] }): React.ReactElement | null {
  const allImages = activities.flatMap((a) => a.imageAttachments ?? [])
  if (allImages.length === 0) return null

  return (
    <div className="flex flex-wrap gap-2 mb-3">
      {allImages.map((img, i) => (
        <InlineImage key={`${img.localPath}-${i}`} attachment={img} />
      ))}
    </div>
  )
}

/**
 * 把流式 agent ToolActivity[] 转换为持久化展示用的 ChatToolActivity[] start/result 配对。
 * 让流式 UI 与历史消息（已用 ChatToolActivityIndicator 渲染）展示风格一致。
 */
function agentActivitiesToChatActivities(activities: ToolActivity[]): import('@/lib/proma-types').ChatToolActivity[] {
  const out: import('@/lib/proma-types').ChatToolActivity[] = []
  for (const a of activities) {
    out.push({
      toolCallId: a.toolUseId,
      type: 'start',
      toolName: a.toolName,
      input: a.input,
    })
    if (a.done) {
      out.push({
        toolCallId: a.toolUseId,
        type: 'result',
        toolName: a.toolName,
        input: a.input,
        result: a.result,
        isError: a.isError,
        status: a.isError ? 'failed' : 'completed',
      })
    }
  }
  return out
}

/** 从持久化事件中提取工具活动列表 */
function extractToolActivities(events: AgentMessage['events']): ToolActivity[] {
  if (!events) return []

  const activities: ToolActivity[] = []
  for (const event of events) {
    if (event.type === 'tool_start') {
      const existingIdx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (existingIdx >= 0) {
        activities[existingIdx] = {
          ...activities[existingIdx]!,
          input: event.input,
          intent: event.intent || activities[existingIdx]!.intent,
          displayName: event.displayName || activities[existingIdx]!.displayName,
        }
      } else {
        activities.push({
          toolUseId: event.toolUseId ?? '',
          toolName: event.toolName ?? '',
          input: event.input,
          intent: event.intent,
          displayName: event.displayName,
          done: true,
          parentToolUseId: event.parentToolUseId,
        })
      }
    } else if (event.type === 'tool_result') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = {
          ...activities[idx]!,
          result: event.result,
          isError: event.isError,
          done: true,
          imageAttachments: event.imageAttachments,
        }
      }
    } else if (event.type === 'task_backgrounded') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, isBackground: true, taskId: event.taskId }
      }
    } else if (event.type === 'shell_backgrounded') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, isBackground: true, shellId: event.shellId }
      }
    } else if (event.type === 'task_progress') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, elapsedSeconds: event.elapsedSeconds }
      }
    } else if (event.type === 'task_started' && event.toolUseId) {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, intent: event.description, taskId: event.taskId }
      }
    }
  }
  return activities
}

/** 解析的附件引用 */
interface AttachedFileRef {
  filename: string
  path: string
}

/** 解析消息中的 <attached_files> 块，返回文件列表和剩余文本 */
function parseAttachedFiles(content: string): { files: AttachedFileRef[]; text: string } {
  const regex = /<attached_files>\n?([\s\S]*?)\n?<\/attached_files>\n*/
  const match = content.match(regex)
  if (!match) return { files: [], text: content }

  const files: AttachedFileRef[] = []
  const lines = match[1]!.split('\n')
  for (const line of lines) {
    // 格式: - filename: /path/to/file
    const lineMatch = line.match(/^-\s+(.+?):\s+(.+)$/)
    if (lineMatch) {
      files.push({ filename: lineMatch[1]!.trim(), path: lineMatch[2]!.trim() })
    }
  }

  const text = content.replace(regex, '').trim()
  return { files, text }
}

/** 判断文件是否为图片类型 */
function isImageFile(filename: string): boolean {
  return /\.(png|jpe?g|gif|webp|svg|bmp|ico)$/i.test(filename)
}

/** 附件引用芯片 */
function AttachedFileChip({ file }: { file: AttachedFileRef }): React.ReactElement {
  const isImg = isImageFile(file.filename)
  const Icon = isImg ? FileImage : FileText

  return (
    <div className="inline-flex items-center gap-1.5 rounded-md bg-muted/60 px-2.5 py-1 text-[12px] text-muted-foreground">
      <Icon className="size-3.5 shrink-0" />
      <span className="truncate max-w-[200px]">{file.filename}</span>
    </div>
  )
}

/** 重试提示组件 - 折叠式 */
function RetryingNotice({ retrying }: { retrying: NonNullable<AgentStreamState['retrying']> }): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const [countdown, setCountdown] = React.useState(0)

  // 倒计时逻辑
  React.useEffect(() => {
    if (retrying.failed || retrying.history.length === 0) {
      setCountdown(0)
      return
    }

    const lastAttempt = retrying.history[retrying.history.length - 1]
    if (!lastAttempt) return

    // 计算倒计时
    const updateCountdown = (): void => {
      const elapsed = (Date.now() - lastAttempt.timestamp) / 1000 // 已过去的秒数
      const remaining = Math.max(0, lastAttempt.delaySeconds - elapsed)
      setCountdown(Math.ceil(remaining))

      if (remaining <= 0) {
        setCountdown(0)
      }
    }

    // 立即更新一次
    updateCountdown()

    // 每 100ms 更新一次倒计时
    const timer = setInterval(updateCountdown, 100)
    return () => clearInterval(timer)
  }, [retrying.failed, retrying.history])

  return (
    <div className="rounded-lg border border-amber-200 bg-amber-50/50 dark:border-amber-800 dark:bg-amber-950/20 p-3 mb-3">
      {/* 头部：简洁状态 */}
      <button
        type="button"
        className="flex items-center gap-2 w-full text-left hover:opacity-80 transition-opacity"
        onClick={() => setExpanded(!expanded)}
      >
        {retrying.failed ? (
          <AlertTriangle className="size-4 text-amber-600 dark:text-amber-400 shrink-0" />
        ) : (
          <RotateCw className="size-4 animate-spin text-amber-600 dark:text-amber-400 shrink-0" />
        )}
        <span className="text-sm text-amber-900 dark:text-amber-100 flex-1">
          {retrying.failed
            ? `重试失败 (${retrying.currentAttempt}/${retrying.maxAttempts})`
            : countdown > 0
              ? `重试倒计时 ${countdown}秒 (${retrying.currentAttempt}/${retrying.maxAttempts})`
              : `重试中 (${retrying.currentAttempt}/${retrying.maxAttempts})`}
          {retrying.history.length > 0 && ` · ${retrying.history[retrying.history.length - 1]?.reason}`}
        </span>
        {expanded ? (
          <ChevronDown className="size-4 text-amber-600 dark:text-amber-400 shrink-0" />
        ) : (
          <ChevronRight className="size-4 text-amber-600 dark:text-amber-400 shrink-0" />
        )}
      </button>

      {/* 展开内容：重试历史 */}
      {expanded && retrying.history.length > 0 && (
        <div className="mt-3 space-y-3 border-t border-amber-200 dark:border-amber-800 pt-3">
          <div className="text-xs font-medium text-amber-900 dark:text-amber-100">
            尝试历史：
          </div>
          {retrying.history.map((attempt, index) => (
            <RetryAttemptItem
              key={attempt.timestamp}
              attempt={attempt}
              isLatest={index === retrying.history.length - 1}
              isFailed={retrying.failed && index === retrying.history.length - 1}
            />
          ))}
          {!retrying.failed && (
            <div className="flex items-center gap-2 text-xs text-amber-700 dark:text-amber-300 pl-6">
              {countdown > 0 ? (
                <>
                  <RotateCw className="size-3 animate-spin" />
                  <span>等待 {countdown} 秒后开始第 {retrying.currentAttempt} 次尝试</span>
                </>
              ) : (
                <>
                  <RotateCw className="size-3 animate-spin" />
                  <span>正在进行第 {retrying.currentAttempt} 次尝试...</span>
                </>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

/** 单条重试尝试记录 */
function RetryAttemptItem({
  attempt,
  isLatest,
  isFailed,
}: {
  attempt: RetryAttempt
  isLatest: boolean
  isFailed: boolean
}): React.ReactElement {
  const [showStderr, setShowStderr] = React.useState(false)
  const [showStack, setShowStack] = React.useState(false)

  const time = new Date(attempt.timestamp).toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })

  return (
    <div className={cn('pl-6 space-y-2', isLatest && 'font-medium')}>
      {/* 尝试头部 */}
      <div className="flex items-start gap-2">
        <span className="text-destructive shrink-0">❌</span>
        <div className="flex-1 min-w-0 space-y-1">
          <div className="text-xs text-amber-900 dark:text-amber-100">
            第 {attempt.attempt} 次 ({time}) - {attempt.reason}
          </div>
          <div className="text-xs text-amber-700 dark:text-amber-300 font-mono break-words">
            {attempt.errorMessage}
          </div>

          {/* 环境信息 */}
          {attempt.environment && (
            <div className="text-[11px] text-amber-600 dark:text-amber-400 space-y-0.5">
              <div>运行时: {attempt.environment.runtime}</div>
              <div>平台: {attempt.environment.platform}</div>
              <div>模型: {attempt.environment.model}</div>
              {attempt.environment.workspace && <div>工作区: {attempt.environment.workspace}</div>}
            </div>
          )}

          {/* 可展开的 stderr */}
          {attempt.stderr && (
            <div className="mt-2">
              <button
                type="button"
                className="text-[11px] text-amber-700 dark:text-amber-300 hover:underline flex items-center gap-1"
                onClick={() => setShowStderr(!showStderr)}
              >
                {showStderr ? (
                  <ChevronDown className="size-3" />
                ) : (
                  <ChevronRight className="size-3" />
                )}
                显示 stderr 输出
              </button>
              {showStderr && (
                <pre className="mt-1 text-[10px] text-amber-800 dark:text-amber-200 bg-amber-100 dark:bg-amber-900/30 p-2 rounded overflow-x-auto max-h-[200px] overflow-y-auto">
                  {attempt.stderr}
                </pre>
              )}
            </div>
          )}

          {/* 可展开的堆栈跟踪 */}
          {attempt.stack && (
            <div className="mt-2">
              <button
                type="button"
                className="text-[11px] text-amber-700 dark:text-amber-300 hover:underline flex items-center gap-1"
                onClick={() => setShowStack(!showStack)}
              >
                {showStack ? (
                  <ChevronDown className="size-3" />
                ) : (
                  <ChevronRight className="size-3" />
                )}
                显示堆栈跟踪
              </button>
              {showStack && (
                <pre className="mt-1 text-[10px] text-amber-800 dark:text-amber-200 bg-amber-100 dark:bg-amber-900/30 p-2 rounded overflow-x-auto max-h-[200px] overflow-y-auto">
                  {attempt.stack}
                </pre>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/** 格式化耗时（毫秒 → 可读字符串） */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const seconds = ms / 1000
  if (seconds < 60) return `${seconds.toFixed(1)}s`
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${m}m ${s.toFixed(0)}s`
}

/** 构建 usage tooltip 多行文本 */
export function buildUsageTooltip(durationMs: number, usage?: AgentEventUsage): string {
  const lines: string[] = []
  lines.push(`耗时: ${formatDuration(durationMs)}`)
  if (usage) {
    const pureInput = usage.inputTokens - (usage.cacheReadTokens ?? 0) - (usage.cacheCreationTokens ?? 0)
    if (pureInput > 0) lines.push(`输入: ${pureInput.toLocaleString()}`)
    if (usage.outputTokens) lines.push(`输出: ${usage.outputTokens.toLocaleString()}`)
    if (usage.cacheCreationTokens) lines.push(`缓存写入: ${usage.cacheCreationTokens.toLocaleString()}`)
    if (usage.cacheReadTokens) lines.push(`缓存读取: ${usage.cacheReadTokens.toLocaleString()}`)
  }
  return lines.join('\n')
}

/** 耗时徽章 — 悬浮显示 token 用量明细（SDKMessageRenderer 复用） */
export function DurationBadge({ durationMs, usage }: { durationMs: number; usage?: AgentEventUsage }): React.ReactElement {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="text-[12px] text-muted-foreground/50 tabular-nums cursor-default hover:text-muted-foreground/70 transition-colors">
          {formatDuration(durationMs)}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p className="whitespace-pre-line text-left">{buildUsageTooltip(durationMs, usage)}</p>
      </TooltipContent>
    </Tooltip>
  )
}

/** 统一消息元信息栏 — 耗时 + token 用量合并为单行，单一 tooltip */
function MessageMetaBar({ durationMs, usage }: { durationMs?: number; usage?: AgentEventUsage }): React.ReactElement | null {
  if (durationMs == null && usage == null) return null

  const parts: string[] = []
  if (durationMs != null) parts.push(formatDuration(durationMs))
  if (usage) {
    const { inputTokens, outputTokens, costUsd } = usage
    parts.push(`${inputTokens.toLocaleString()} 输入`)
    parts.push(`${(outputTokens ?? 0).toLocaleString()} 输出`)
    if (costUsd != null && costUsd > 0) parts.push(`$${costUsd.toFixed(4)}`)
  }

  const tooltipText = durationMs != null ? buildUsageTooltip(durationMs, usage) : null

  const content = (
    <span className="inline-flex items-center gap-1 text-[12px] text-muted-foreground/50 tabular-nums cursor-default hover:text-muted-foreground/70 transition-colors animate-in fade-in duration-300">
      <Zap size={11} strokeWidth={2} className="shrink-0" />
      {parts.map((p, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span className="opacity-40">·</span>}
          <span>{p}</span>
        </React.Fragment>
      ))}
    </span>
  )

  if (!tooltipText) return content

  return (
    <Tooltip>
      <TooltipTrigger asChild>{content}</TooltipTrigger>
      <TooltipContent side="top">
        <p className="whitespace-pre-line text-left">{tooltipText}</p>
      </TooltipContent>
    </Tooltip>
  )
}

/** 相对时间戳 — 简化显示，如 "2m ago" / "刚刚" */
function formatRelativeShort(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return new Date(ts).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })
}

/** AgentMessageItem 属性接口 */
interface AgentMessageItemProps {
  message: AgentMessage
  sessionPath?: string | null
  attachedDirs?: string[]
}

function AgentMessageItem({ message, sessionPath, attachedDirs }: AgentMessageItemProps): React.ReactElement | null {
  const userProfile = useAtomValue(userProfileAtom)
  const channels = useAtomValue(channelsAtom)

  if (message.role === 'user') {
    const { files: attachedFiles, text: messageText } = parseAttachedFiles(message.content)

    return (
      <Message from="user">
        <div className="flex items-start gap-2.5 mb-2.5">
          <UserAvatar avatar={userProfile.avatar} size={35} />
          <div className="flex flex-col justify-between h-[35px]">
            <span className="text-sm font-semibold text-foreground/60 leading-none">{userProfile.userName}</span>
            <span className="text-[10px] text-foreground/[0.38] leading-none">{formatMessageTime(message.createdAt)}</span>
          </div>
        </div>
        <MessageContent>
          {attachedFiles.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mb-2">
              {attachedFiles.map((file) => (
                <AttachedFileChip key={file.path} file={file} />
              ))}
            </div>
          )}
          {messageText && (
            <UserMessageContent>{messageText}</UserMessageContent>
          )}
        </MessageContent>
        {/* 操作按钮（hover 时可见） */}
        {messageText && (
          <MessageActions className="pl-[46px] mt-0.5">
            <CopyButton content={messageText} />
          </MessageActions>
        )}
      </Message>
    )
  }

  if (message.role === 'assistant') {
    const toolActivities = extractToolActivities(message.events)
    // Parse skill citations once — used both to clean the body for
    // markdown render and to drive the chip row below MessageContent.
    const parsed = message.content
      ? parseSkillCitations(message.content)
      : { cleanedContent: '', citations: [] }

    return (
      <Message from="assistant">
        <MessageHeader
          model={message.model ? resolveModelDisplayName(message.model, channels) : undefined}
          time={formatMessageTime(message.createdAt)}
          logo={<AssistantLogo model={message.model} />}
        />
        <MessageContent>
          {message.contentBlocks && message.contentBlocks.length > 0 ? (
            <NativeBlockRenderer
              blocks={message.contentBlocks}
              conversationId={message.sessionId}
            />
          ) : (
            <>
              {/* 历史消息的 thinking block — 从持久化的 reasoning 字段渲染 */}
              {message.reasoning && (
                <div className="mb-3">
                  <ThinkingBlock
                    block={{ type: 'thinking', thinking: message.reasoning } as any}
                    dimmed={false}
                  />
                </div>
              )}
              {/* 历史消息工具调用 — 优先用 message.toolActivities（chat 格式，PR #5 持久化的）；
                  若为空则把从 events 提取的 agent 格式转换为 chat 格式，统一用
                  ChatToolActivityIndicator 渲染（🔧 工具名 + 折叠结果卡片）。 */}
              {(message.toolActivities && message.toolActivities.length > 0) ? (
                <div className="mb-3">
                  <ChatToolActivityIndicator activities={message.toolActivities} />
                </div>
              ) : toolActivities.length > 0 ? (
                <div className="mb-3">
                  <ChatToolActivityIndicator activities={agentActivitiesToChatActivities(toolActivities)} />
                </div>
              ) : null}
              <ToolResultInlineImages activities={toolActivities} />
              {message.content && (
                <MessageResponse basePath={sessionPath || undefined} basePaths={attachedDirs}>{parsed.cleanedContent}</MessageResponse>
              )}
            </>
          )}
        </MessageContent>
        {/* Skill citation chips — sibling of MessageActions so the
            pl-[46px] indent aligns with the avatar gutter. */}
        <SkillCitationChips citations={parsed.citations} messageKey={message.id} />
        {/* 操作栏：左侧靠左排列 */}
        {(message.durationMs != null || message.usage || message.content) && (
          <MessageActions className="pl-[46px] mt-0.5 justify-start gap-2.5">
            <MessageMetaBar durationMs={message.durationMs ?? undefined} usage={message.usage ?? undefined} />
            {message.content && <CopyButton content={message.content} />}
            <span className="text-[12px] text-muted-foreground/40 tabular-nums">
              {formatRelativeShort(message.createdAt)}
            </span>
          </MessageActions>
        )}
      </Message>
    )
  }

  return null
}

/**
 * Agent 运行指示器 — 3 段式流转点 + 运行时间 + 可选累积统计。
 *
 * Layer 3 of the agent status visibility upgrade. Lives inside the
 * streaming bubble (in-conversation cursor of "this turn is in progress").
 * Distinct visual rhythm from AgentStatusBar (which is the persistent
 * sticky bar above input).
 *
 * Visual: 3 dots cascade-pulsing left→right (ChatGPT-style typing
 * indicator), accent-colored, more modern than a generic spinner.
 */
function AgentRunningIndicator({
  startedAt,
  toolCount,
  inputTokens,
  outputTokens,
}: {
  startedAt?: number
  toolCount?: number
  inputTokens?: number
  outputTokens?: number
}): React.ReactElement {
  const [elapsed, setElapsed] = React.useState(0)

  React.useEffect(() => {
    const start = startedAt ?? Date.now()
    const update = (): void => setElapsed((Date.now() - start) / 1000)
    update()
    const timer = setInterval(update, 100)
    return () => clearInterval(timer)
  }, [startedAt])

  const formatTime = (seconds: number): string => {
    if (seconds < 60) return `${seconds.toFixed(1)}s`
    const m = Math.floor(seconds / 60)
    const s = seconds % 60
    return `${m}m ${s.toFixed(1)}s`
  }
  const formatTokens = (n?: number): string => {
    if (!n) return '0'
    if (n < 1000) return String(n)
    return `${(n / 1000).toFixed(1)}k`
  }

  const showStats = (toolCount ?? 0) > 0 || !!inputTokens || !!outputTokens

  return (
    <div className="flex items-center gap-2 min-h-[28px]">
      {/* 3-dot cascading pulse — modern typing-indicator style */}
      <span className="flex items-center gap-[3px]" aria-label="正在执行">
        <span className="size-1.5 rounded-full bg-primary/75 animate-pulse" style={{ animationDelay: '0ms', animationDuration: '1200ms' }} />
        <span className="size-1.5 rounded-full bg-primary/75 animate-pulse" style={{ animationDelay: '200ms', animationDuration: '1200ms' }} />
        <span className="size-1.5 rounded-full bg-primary/75 animate-pulse" style={{ animationDelay: '400ms', animationDuration: '1200ms' }} />
      </span>
      <span className="text-[13px] font-light text-muted-foreground/75 tabular-nums">
        Agent Running {formatTime(elapsed)}
      </span>
      {showStats && (
        <span className="text-[11px] text-muted-foreground/50 tabular-nums">
          {(toolCount ?? 0) > 0 && <>· {toolCount} 工具</>}
          {(inputTokens || outputTokens) && (
            <> · ↑{formatTokens(inputTokens)} ↓{formatTokens(outputTokens)}</>
          )}
        </span>
      )}
    </div>
  )
}

export function AgentMessages({ sessionId, sessionModelId, messages, messagesLoaded, streaming, streamState, liveMessages, sessionPath, attachedDirs, stoppedByUser, onRetry, onRetryInNewSession, onFork, onRewind, onCompact }: AgentMessagesProps): React.ReactElement {
  const userProfile = useAtomValue(userProfileAtom)
  const setMinimapCache = useSetAtom(tabMinimapCacheAtom)
  const channels = useAtomValue(channelsAtom)
  const proactiveLearningEvents = useAtomValue(proactiveLearningEventsAtom)
  /** 淡入控制：切换会话时先隐藏，等布局完成后再显示。 */
  const [ready, setReady] = React.useState(false)
  const prevSessionIdRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (sessionId !== prevSessionIdRef.current) {
      prevSessionIdRef.current = sessionId
      setReady(false)
    }
  }, [sessionId])

  React.useEffect(() => {
    if (ready) return

    // 必须等消息加载完成，否则 messages=[] 会被误判为空对话
    if (messagesLoaded === false) return

    // 流式进行中且有实时内容 → 跳过 fade 直接显示
    if (streaming && liveMessages && liveMessages.length > 0) {
      setReady(true)
      return
    }

    if (messages.length === 0 && !streaming) {
      setReady(true)
      return
    }
    let cancelled = false
    requestAnimationFrame(() => {
      if (!cancelled) setReady(true)
    })
    return () => { cancelled = true }
  }, [messages, streaming, liveMessages, messagesLoaded])

  // 从 streamState 属性中计算派生值
  const streamingContent = streamState?.content ?? ''
  const agentStreamingModel = streamState?.model ? resolveModelDisplayName(streamState.model, channels) : undefined
  const retrying = streamState?.retrying
  const startedAt = streamState?.startedAt

  const { displayedContent: rawSmoothContent } = useSmoothStream({
    content: streamingContent,
    isStreaming: streaming,
  })

  // 防闪屏守卫：useSmoothStream 通过 useEffect 重置 displayedContent，比 render 晚一帧。
  // 当 streamingContent 已清空但 smoothContent 仍持有旧值时，
  // 会导致 fallback 气泡与持久化消息同时渲染一帧（重复内容闪烁）。
  // 用原始 streamingContent 作为守卫：内容已清空且不在流式中，立即归零。
  const smoothContent = (streaming || streamingContent) ? rawSmoothContent : ''

  /**
   * 流式完成过渡：streaming 结束到持久化消息加载完成之间，
   * 强制 resize="instant" 避免中间高度变化触发平滑滚动动画。
   *
   * 使用 render-phase 计算避免 useEffect 延迟一帧的问题：
   * - streaming 变 false 的第一帧就能立即切到 instant，防止闪动
   * - 后续通过 ref+timeout 延迟 150ms 才允许切回 smooth
   */
  const [transitioningCooldown, setTransitioningCooldown] = React.useState(false)
  const wasStreamingRef = React.useRef(streaming)

  // render-phase 判断：是否处于需要 instant resize 的过渡期
  // liveMessages 非空说明持久化消息还没加载完（加载完后会清空 liveMessages）
  const needsInstant = !streaming && (!!streamingContent || !!smoothContent || (liveMessages != null && liveMessages.length > 0))

  React.useEffect(() => {
    // 刚从 streaming → not-streaming：启动 cooldown
    if (wasStreamingRef.current && !streaming) {
      setTransitioningCooldown(true)
    }
    wasStreamingRef.current = streaming
  }, [streaming])

  React.useEffect(() => {
    if (needsInstant) return
    // 过渡完成后延迟 150ms 才关闭 cooldown，给 StickToBottom 时间稳定
    const timer = setTimeout(() => setTransitioningCooldown(false), 150)
    return () => clearTimeout(timer)
  }, [needsInstant])

  const transitioning = needsInstant || transitioningCooldown

  const hasContent = messages.length > 0

  // 压缩流程进行中（含收尾窗口：compact_boundary 已到但 result 未到）
  // → 一律抑制 AgentRunningIndicator，避免压缩分隔符切换期间闪烁。
  // compactInFlight 从点击压缩 / SDK compacting 事件开始为 true，
  // 直到整个 stream 结束（stream state 被删除）才消失。
  const suppressAgentRunning = streamState?.isCompacting || streamState?.compactInFlight

  // 迷你地图数据
  const minimapItems: MinimapItem[] = React.useMemo(
    () => {
      return messages.map((m, i) => ({
        id: m.id || `msg-${i}`,
        role: m.role === 'status' ? 'status' as const : m.role as MinimapItem['role'],
        preview: (m.content ?? '').replace(/<attached_files>[\s\S]*?<\/attached_files>\n*/, '').slice(0, 200),
        avatar: m.role === 'user' ? userProfile.avatar : undefined,
        model: m.model,
      }))
    },
    [messages, userProfile.avatar]
  )

  // 同步 minimap 缓存到 Tab 级别（供 Tab hover 预览使用）
  React.useEffect(() => {
    if (minimapItems.length > 0) {
      setMinimapCache((prev) => {
        const next = new Map(prev)
        next.set(sessionId, minimapItems.map(item => ({ ...item, avatar: item.avatar ?? undefined })))
        return next
      })
    }
  }, [sessionId, minimapItems, setMinimapCache])

  // 所有用户消息的数据 — 供 StickyUserMessage 使用
  const allUserMessagesData = React.useMemo(() => {
    return messages
      .filter((m) => m.role === 'user')
      .map((m) => {
        const { files, text } = parseAttachedFiles(m.content ?? '')
        return {
          id: m.id ?? null,
          text,
          attachments: files.map((f) => ({ filename: f.filename, isImage: isImageFile(f.filename) })),
        }
      })
  }, [messages])

  return (
    <BasePathsProvider basePaths={attachedDirs}>
    <Conversation resize={ready && !transitioning ? 'smooth' : 'instant'} className={ready ? 'opacity-100 transition-opacity duration-200' : 'opacity-0'}>
      <ScrollPositionManager id={sessionId} ready={ready} />
      <ConversationContent>
        {!hasContent && !streaming ? (
          <EmptyState />
        ) : (
          <>
            {/* 消息渲染 — AgentMessageItem */}
            {messages.map((msg: AgentMessage) => (
              <div key={msg.id} data-message-id={msg.id} data-message-role={msg.role === 'user' ? 'user' : undefined}>
                <AgentMessageItem
                  message={msg}
                  sessionPath={sessionPath}
                  attachedDirs={attachedDirs}
                />
              </div>
            ))}

            {/* 流式气泡（含头像/名称/时间） */}
            {!suppressAgentRunning && (streaming || smoothContent || retrying || (streamState?.toolActivities?.length ?? 0) > 0 || streamState?.reasoning) && (
              <Message from="assistant">
                <MessageHeader
                  model={agentStreamingModel}
                  time={formatMessageTime(Date.now())}
                  logo={<AssistantLogo model={agentStreamingModel} />}
                />
                <MessageContent>
                  {retrying && <RetryingNotice retrying={retrying} />}
                  {(streamState?.reasoning) && (
                    <div className="mb-3">
                      <ThinkingBlock block={{ type: 'thinking', thinking: streamState.reasoning } as any} dimmed={!!smoothContent} />
                    </div>
                  )}
                  {(streamState?.toolActivities?.length ?? 0) > 0 && (
                    <div className="mb-3">
                      {/* 流式工具调用 — 转成 ChatToolActivity 后用 ChatToolActivityIndicator 渲染，
                          视觉与历史消息保持一致（ChatToolBlock 的 🔧 toolName + 折叠结果卡片样式） */}
                      <ChatToolActivityIndicator
                        activities={agentActivitiesToChatActivities(streamState!.toolActivities)}
                        isStreaming={streaming}
                      />
                    </div>
                  )}
                  {smoothContent ? (() => {
                    const { cleanedContent: streamCleanedContent, citations: streamCitations } = parseSkillCitations(smoothContent)
                    return (
                      <>
                        <MessageResponse basePath={sessionPath || undefined} basePaths={attachedDirs}>{streamCleanedContent}</MessageResponse>
                        {/* Once the citation block has fully streamed in, render
                            the chip(s) — the dedupe key uses the session id so
                            the streaming chip and the post-finalization chip
                            don't both bump cited_count. */}
                        <SkillCitationChips
                          citations={streamCitations}
                          messageKey={`stream-${sessionId}`}
                        />
                        {streaming && (
                          <AgentRunningIndicator
                            startedAt={startedAt}
                            toolCount={streamState?.toolActivities?.length}
                            inputTokens={streamState?.inputTokens}
                            outputTokens={streamState?.outputTokens}
                          />
                        )}
                      </>
                    )
                  })() : (
                    streaming && (
                      <AgentRunningIndicator
                        startedAt={startedAt}
                        toolCount={streamState?.toolActivities?.length}
                        inputTokens={streamState?.inputTokens}
                        outputTokens={streamState?.outputTokens}
                      />
                    )
                  )}
                </MessageContent>
                {/* 流式完成后显示 token 用量 */}
                {!streaming && smoothContent && streamState?.inputTokens != null && (
                  <MessageActions className="pl-[46px] mt-0.5 justify-start gap-2.5">
                    <MessageMetaBar usage={{
                      inputTokens: streamState.inputTokens,
                      outputTokens: streamState.outputTokens,
                      costUsd: streamState.costUsd,
                    }} />
                  </MessageActions>
                )}
              </Message>
            )}

            {/* 压缩中指示器：由 isCompacting flag 驱动的尾部元素，compact_boundary 到达时 flag 翻 false 自然消失，
                视觉上被流中新出现的"上下文已压缩"分隔符无缝替换 */}
            {streamState?.isCompacting && <CompactingIndicator />}

          </>
        )}
      </ConversationContent>
      {/* 记忆捕捉 chip 列表 — 显示最近的 3 条 */}
      {proactiveLearningEvents.length > 0 && (
        <div className="flex flex-wrap gap-2 px-4 pb-2">
          {proactiveLearningEvents.slice(0, 3).map((ev) => (
            <ProactiveLearningChip key={ev.timestamp} event={ev} />
          ))}
        </div>
      )}
      <ScrollMinimap items={minimapItems} />
      <ConversationScrollButton />
      {allUserMessagesData.length > 0 && (
        <StickyUserMessage userMessages={allUserMessagesData} />
      )}
    </Conversation>
    </BasePathsProvider>
  )
}

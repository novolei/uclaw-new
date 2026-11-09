/**
 * ContextUsageBadge — 上下文使用量指示器
 *
 * 输入框工具栏上的一个 36×36 圆形按钮：
 * - 内部为 16px 圆环，按 displayTokens / displayWindow 比例渲染
 * - hover / click 弹出 Popover，内含 token 明细 + 手动压缩按钮
 * - 压缩中时按钮位置显示 Loader2 旋转图标
 * - 占用接近压缩阈值（后端阈值，缺省为窗口 × 0.775）的 80% 时圆环变琥珀色
 * - 无数据时不显示
 * - transcript 会话（系统提示词已冻结）额外显示"提示词缓存"分区
 */

import * as React from 'react'
import { Loader2, Minimize2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import type { FrameRebuildReason, MemoryInjection } from '@/lib/types'

/** 压缩阈值比例（SDK 在 ~77.5% 窗口大小时自动压缩） */
const COMPACT_THRESHOLD_RATIO = 0.775
/** 显示警告的阈值（压缩阈值的 80%） */
const WARNING_RATIO = 0.80
/** Popover hover 关闭延迟（ms），与 AgentThinkingPopover 一致 */
const HOVER_CLOSE_DELAY = 150

/** 框架重建原因 → 系统提示词行副文本（'new' 是首次创建，不提示） */
const REBUILD_REASON_TEXT: Partial<Record<FrameRebuildReason, string>> = {
  prompt_changed: '本会话已重建：提示词模板变化',
  workspace_changed: '本会话已重建：工作区变化',
  version_changed: '本会话已重建：应用升级',
}

const MEMORY_INJECTION_TEXT: Record<MemoryInjection, string> = {
  full: '记忆：完整',
  delta: '记忆：增量',
  none: '记忆：无',
}

interface ContextUsageBadgeProps {
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  /** Skill manifest token cost (from agent:context_stats). Shows a "技能" row
   *  in the popover when > 0. */
  skillsTokens?: number
  /** Effective auto-compaction threshold (from agent:context_stats; only for
   *  transcript-mode sessions). Falls back to window × COMPACT_THRESHOLD_RATIO. */
  compactionThresholdTokens?: number
  /** This LLM call's prompt-cache hit ratio (0..1) from agent:turn_cost,
   *  computed by the backend per wire protocol. Absent → no ratio shown. */
  cacheHitRatio?: number
  /** Wire cache semantics of `inputTokens` (agent:turn_cost): `false` =
   *  Anthropic Messages (input excludes cache reads and writes → prompt
   *  total = input + reads + writes); `true`/absent = input is the total. */
  inputIncludesCache?: boolean
  /** Transcript mode only (agent:context_stats): the system prompt is the
   *  session's frozen frame. Gates the "提示词缓存" section. */
  frozenSystemPrompt?: boolean
  /** Frozen frame prompt estimate (transcript mode). */
  systemPromptTokens?: number
  toolDefTokens?: number
  toolCount?: number
  frameRebuildReason?: FrameRebuildReason
  memoryInjection?: MemoryInjection
  turnContextTokens?: number
  /** Session-wide hit ratio (0..1) from get_session_cache_stats. */
  sessionCacheHitRatio?: number
  /** The provider has never reported cache reads for this session
   *  (calls > 0, cachedInputTokens == 0) → show "未上报" instead of 0%. */
  sessionCacheUnreported?: boolean
  isCompacting: boolean
  isProcessing: boolean
  onCompact: () => void
}

/** 格式化 token 数为可读字符串（如 1234 → "1.2k"） */
function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`
  }
  if (tokens >= 1_000) {
    return `${(tokens / 1_000).toFixed(1)}k`
  }
  return `${tokens}`
}

/** 圆环进度指示器 — 16×16 SVG，描边 2px */
interface UsageRingProps {
  ratio: number
  isWarning: boolean
}
function UsageRing({ ratio, isWarning }: UsageRingProps): React.ReactElement {
  const radius = 8
  const circumference = 2 * Math.PI * radius
  const clamped = Math.max(0, Math.min(1, ratio))
  const dashOffset = circumference * (1 - clamped)

  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      className={cn(
        'shrink-0 transition-colors',
        isWarning ? 'text-amber-500 dark:text-amber-400' : 'text-foreground/70',
      )}
      aria-hidden="true"
    >
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeOpacity="0.2"
        strokeWidth="2"
      />
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={dashOffset}
        transform="rotate(-90 10 10)"
        style={{ transition: 'stroke-dashoffset 300ms ease-out' }}
      />
    </svg>
  )
}

/** Popover 里的一行 key/value */
interface DetailRowProps {
  label: string
  // Widened so the "context" row can embed a "1M" badge alongside the
  // numeric value — earlier this was string-only.
  value: React.ReactNode
  emphasized?: boolean
  /** Optional muted line under the row (e.g. why the frame was rebuilt). */
  hint?: string
}
function DetailRow({ label, value, emphasized, hint }: DetailRowProps): React.ReactElement {
  const row = (
    <div className="flex items-center justify-between gap-4 text-xs">
      <span className="text-foreground/70">{label}</span>
      <span className={cn('tabular-nums', emphasized ? 'font-medium text-foreground' : 'text-foreground/90')}>
        {value}
      </span>
    </div>
  )
  if (!hint) return row
  return (
    <div className="flex flex-col gap-0.5">
      {row}
      <span className="text-[10.5px] text-amber-600/90 dark:text-amber-400/90">{hint}</span>
    </div>
  )
}

/** 命中率徽标配色：≥50% 绿，>0 琥珀，0 灰 */
function hitBadgeClass(percent: number): string {
  return percent >= 50
    ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
    : percent > 0
    ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
    : 'bg-muted text-muted-foreground/60'
}

export function ContextUsageBadge({
  inputTokens,
  outputTokens,
  cacheReadTokens,
  cacheCreationTokens,
  costUsd,
  contextWindow,
  skillsTokens,
  compactionThresholdTokens,
  cacheHitRatio,
  inputIncludesCache,
  frozenSystemPrompt,
  systemPromptTokens,
  toolDefTokens,
  toolCount,
  frameRebuildReason,
  memoryInjection,
  turnContextTokens,
  sessionCacheHitRatio,
  sessionCacheUnreported,
  isCompacting,
  isProcessing,
  onCompact,
}: ContextUsageBadgeProps): React.ReactElement | null {
  // 保留最近一次有效的 token 值，避免切换会话时闪烁消失
  const stableRef = React.useRef<{
    inputTokens: number
    outputTokens?: number
    cacheReadTokens?: number
    cacheCreationTokens?: number
    costUsd?: number
    contextWindow?: number
    skillsTokens?: number
    compactionThresholdTokens?: number
    cacheHitRatio?: number
    inputIncludesCache?: boolean
  } | null>(null)
  if (inputTokens && inputTokens > 0) {
    stableRef.current = {
      inputTokens, outputTokens, cacheReadTokens, cacheCreationTokens, costUsd, contextWindow, skillsTokens,
      compactionThresholdTokens, cacheHitRatio, inputIncludesCache,
    }
  }

  const [open, setOpen] = React.useState(false)
  const closeTimerRef = React.useRef<number | null>(null)

  const cancelClose = React.useCallback(() => {
    if (closeTimerRef.current != null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }, [])

  const scheduleClose = React.useCallback(() => {
    cancelClose()
    closeTimerRef.current = window.setTimeout(() => setOpen(false), HOVER_CLOSE_DELAY)
  }, [cancelClose])

  React.useEffect(() => cancelClose, [cancelClose])

  // 压缩中 → 按钮位置显示 spinner
  if (isCompacting) {
    return (
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="size-[36px] rounded-full text-muted-foreground cursor-default"
        disabled
      >
        <Loader2 className="size-4 animate-spin" />
      </Button>
    )
  }

  // 使用稳定值：优先当前数据，回退到上次有效数据
  const stable = stableRef.current
  const hasCurrent = inputTokens != null && inputTokens > 0
  const displayInput = hasCurrent ? inputTokens : stable?.inputTokens
  const displayWindow = hasCurrent ? contextWindow : stable?.contextWindow
  const displayOutput = hasCurrent ? outputTokens : stable?.outputTokens
  const displayCacheRead = hasCurrent ? cacheReadTokens : stable?.cacheReadTokens
  const displayCacheCreation = hasCurrent ? cacheCreationTokens : stable?.cacheCreationTokens
  // Anthropic Messages wire: inputTokens excludes cache reads AND writes, so
  // the prompt total is their sum. OpenAI-compatible / unknown: inputTokens
  // already is the prompt total (cache reads included, writes not reported).
  const anthropicWire = (hasCurrent ? inputIncludesCache : stable?.inputIncludesCache) === false
  const displayTokens = displayInput != null && anthropicWire
    ? displayInput + (displayCacheRead ?? 0) + (displayCacheCreation ?? 0)
    : displayInput
  const displayCost = hasCurrent ? costUsd : stable?.costUsd
  // skillsTokens comes in via the agent:context_stats event, which fires
  // independently of usage_update. Always prefer the current prop value
  // (don't gate on hasCurrent); fall back to stable cache for session switch.
  const displaySkills = skillsTokens != null ? skillsTokens : stable?.skillsTokens
  // Like skillsTokens, the threshold arrives via agent:context_stats.
  const displayThreshold = compactionThresholdTokens != null
    ? compactionThresholdTokens
    : stable?.compactionThresholdTokens

  // 从未有过 usage 数据 → 不显示
  if (!displayTokens || displayTokens <= 0) return null

  // 警告阈值：基于压缩阈值的 80%。优先使用后端实际压缩阈值，
  // 缺省回退到 contextWindow × 0.775。
  const compactThreshold = displayThreshold ?? (displayWindow
    ? Math.floor(displayWindow * COMPACT_THRESHOLD_RATIO)
    : undefined)
  const remainingToCompact = compactThreshold != null
    ? Math.max(0, compactThreshold - displayTokens)
    : undefined
  const isWarning = compactThreshold
    ? displayTokens / compactThreshold >= WARNING_RATIO
    : false

  const ratio = displayWindow ? displayTokens / displayWindow : 0

  // 纯输入：Anthropic 线上 input 本身就是新输入；其余 = 总上下文 - 缓存读取 - 缓存写入
  const pureInput = anthropicWire
    ? (displayInput ?? 0)
    : displayTokens - (displayCacheRead ?? 0) - (displayCacheCreation ?? 0)

  const percent = displayWindow
    ? Math.round((displayTokens / displayWindow) * 100)
    : undefined

  // 本次调用缓存命中率 — 只用后端按线上协议算好的值（input 是否含缓存读取
  // 因服务商而异，前端自算会在 OpenAI 兼容服务商上重复计数）。
  const displayHitRatio = hasCurrent ? cacheHitRatio : stable?.cacheHitRatio
  const callHitPercent = displayHitRatio != null ? Math.round(displayHitRatio * 100) : undefined
  // A call that read or wrote the cache proves the provider reports cache
  // usage — a cold call (write only, 0 reads) is 0%, not "未上报".
  const cacheReported = (displayCacheRead ?? 0) > 0 || (displayCacheCreation ?? 0) > 0
  const showUnreported = sessionCacheUnreported === true && !cacheReported
  const coldWriteOnly = (displayCacheCreation ?? 0) > 0 && (displayCacheRead ?? 0) === 0

  // "提示词缓存"分区只在 transcript 会话（冻结系统提示词）出现
  const showPromptCache = frozenSystemPrompt === true
  const rebuildHint = frameRebuildReason ? REBUILD_REASON_TEXT[frameRebuildReason] : undefined
  const turnContextParts: string[] = []
  if (turnContextTokens != null) turnContextParts.push(formatTokens(turnContextTokens))
  if (memoryInjection) turnContextParts.push(MEMORY_INJECTION_TEXT[memoryInjection])
  const sessionHitPercent = sessionCacheHitRatio != null
    ? Math.round(sessionCacheHitRatio * 100)
    : undefined

  // 等效"省下的"输入 token 估算：缓存读取以 10% 输入价计费（Anthropic 当前
  // 比例），所以省下的等效 input token ≈ cacheRead × 0.9。Anthropic 线上缓存
  // 写入按 125% 计费，再扣掉 cacheCreation × 0.25。粗略展示用。
  const cacheWritePremium = anthropicWire ? (displayCacheCreation ?? 0) * 0.25 : 0
  const cacheSavedInput = Math.max(0, Math.round((displayCacheRead ?? 0) * 0.9 - cacheWritePremium))

  // 货币格式：$0.0023 / $0.123 / $1.23 — 自适应小数位
  const formatCost = (usd: number): string => {
    if (usd >= 1) return `$${usd.toFixed(2)}`
    if (usd >= 0.01) return `$${usd.toFixed(3)}`
    return `$${usd.toFixed(4)}`
  }

  const handleCompactClick = (): void => {
    if (isProcessing) return
    onCompact()
    setOpen(false)
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className={cn(
            'size-[36px] rounded-full',
            isWarning ? 'text-amber-600 dark:text-amber-400' : 'text-foreground/60 hover:text-foreground',
          )}
          onMouseEnter={() => {
            cancelClose()
            setOpen(true)
          }}
          onMouseLeave={scheduleClose}
        >
          <UsageRing ratio={ratio} isWarning={isWarning} />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="center"
        sideOffset={8}
        className="w-auto min-w-[260px] p-2.5"
        onMouseEnter={cancelClose}
        onMouseLeave={scheduleClose}
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        <div className="flex flex-col gap-2">
          {/* 输入构成 */}
          <div className="flex flex-col gap-1">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
              输入构成
            </div>
            {pureInput > 0 && <DetailRow label="新输入" value={pureInput.toLocaleString()} />}
            {displayCacheCreation ? (
              <DetailRow label="缓存写入" value={displayCacheCreation.toLocaleString()} />
            ) : null}
            {displayCacheRead ? (
              <DetailRow
                label="缓存读取"
                value={
                  <span className="inline-flex items-center gap-1.5">
                    <span>{displayCacheRead.toLocaleString()}</span>
                    {callHitPercent != null && callHitPercent > 0 && (
                      <span
                        className="rounded-sm bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 px-1 py-0 text-[9.5px] font-semibold tabular-nums"
                        title={`本次调用 ${callHitPercent}% 输入命中缓存，约省 ${cacheSavedInput.toLocaleString()} 输入 token`}
                      >
                        {callHitPercent}% 命中
                      </span>
                    )}
                  </span>
                }
              />
            ) : null}
            {displayOutput ? <DetailRow label="输出" value={displayOutput.toLocaleString()} /> : null}
            {displaySkills != null && displaySkills > 0 ? (
              <DetailRow label="技能 manifest" value={displaySkills.toLocaleString()} />
            ) : null}
          </div>

          {/* 缓存效率 — 有后端命中率时显示（冷启动 0% 方便监控） */}
          {callHitPercent != null || cacheSavedInput > 0 ? (
          <>
            <div className="h-px bg-border" />
            <div className="flex flex-col gap-1">
              <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                缓存效率
              </div>
              {callHitPercent != null && (
                <DetailRow
                  label="本次调用命中"
                  value={
                    <span className="inline-flex items-center gap-1.5">
                      <span className="tabular-nums">{(displayCacheRead ?? 0).toLocaleString()}</span>
                      {showUnreported ? (
                        <span className="text-muted-foreground/60" title="当前服务商未返回缓存读取数据">
                          未上报
                        </span>
                      ) : coldWriteOnly ? (
                        <span className="text-muted-foreground/70" title="本次调用写入了缓存，后续调用可命中">
                          0% · 缓存已写入
                        </span>
                      ) : (
                        <span
                          className={cn(
                            'rounded-sm px-1 py-0 text-[9.5px] font-semibold tabular-nums',
                            hitBadgeClass(callHitPercent),
                          )}
                        >
                          {callHitPercent}%
                        </span>
                      )}
                    </span>
                  }
                  emphasized
                />
              )}
              {(displayCacheCreation ?? 0) > 0 && (
                <DetailRow
                  label="缓存写入"
                  value={
                    <span className="text-muted-foreground/80 tabular-nums">
                      {(displayCacheCreation ?? 0).toLocaleString()}
                    </span>
                  }
                />
              )}
              {cacheSavedInput > 0 && (
                <DetailRow
                  label="节省约"
                  value={
                    <span className="text-emerald-600 dark:text-emerald-400 tabular-nums">
                      ~{cacheSavedInput.toLocaleString()} token
                    </span>
                  }
                />
              )}
            </div>
          </>
          ) : null}

          {/* 提示词缓存 — transcript 会话：冻结系统提示词 + 工具定义 + 每轮动态块 */}
          {showPromptCache ? (
            <>
              <div className="h-px bg-border" />
              <div className="flex flex-col gap-1">
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                  提示词缓存
                </div>
                {systemPromptTokens != null && (
                  <DetailRow
                    label="系统提示词"
                    value={`${formatTokens(systemPromptTokens)} · 已冻结`}
                    hint={rebuildHint}
                  />
                )}
                {toolCount != null && (
                  <DetailRow
                    label="工具定义"
                    value={`${toolCount} 个 · ${formatTokens(toolDefTokens ?? 0)}`}
                  />
                )}
                {turnContextParts.length > 0 && (
                  <DetailRow label="本轮动态块" value={turnContextParts.join(' · ')} />
                )}
                {showUnreported ? (
                  <DetailRow
                    label="本会话命中率"
                    value={
                      <span
                        className="text-muted-foreground/60"
                        title="当前服务商未返回缓存读取数据，无法计算命中率"
                      >
                        未上报
                      </span>
                    }
                  />
                ) : sessionHitPercent != null ? (
                  <DetailRow
                    label="本会话命中率"
                    value={
                      <span
                        className={cn(
                          'rounded-sm px-1 py-0 text-[10px] font-semibold tabular-nums',
                          hitBadgeClass(sessionHitPercent),
                        )}
                      >
                        {sessionHitPercent}%
                      </span>
                    }
                    emphasized
                  />
                ) : null}
              </div>
            </>
          ) : null}

          {/* 上下文窗口 */}
          {displayWindow ? (
            <>
              <div className="h-px bg-border" />
              <div className="flex flex-col gap-1">
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                  上下文窗口
                </div>
                <DetailRow
                  label="已用 / 总量"
                  value={
                    <span className="inline-flex items-center gap-1.5">
                      {formatTokens(displayTokens)} / {formatTokens(displayWindow)}
                      {displayWindow >= 1_000_000 && (
                        <span
                          className="rounded-sm bg-primary/15 text-primary px-1 py-0 text-[9.5px] font-semibold uppercase tracking-wider"
                          title="1M context window beta enabled for this model"
                        >
                          1M
                        </span>
                      )}
                    </span>
                  }
                  emphasized
                />
                {percent != null && (
                  <DetailRow
                    label="占用"
                    value={`${percent}%`}
                    emphasized={isWarning}
                  />
                )}
                {remainingToCompact != null && (
                  <DetailRow
                    label="距离自动压缩"
                    value={formatTokens(remainingToCompact)}
                    emphasized={isWarning}
                  />
                )}
              </div>
            </>
          ) : null}

          {/* 费用 */}
          {displayCost != null && displayCost > 0 ? (
            <>
              <div className="h-px bg-border" />
              <div className="flex flex-col gap-1">
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                  本轮成本
                </div>
                <DetailRow label="USD" value={formatCost(displayCost)} emphasized />
              </div>
            </>
          ) : null}

          <div className="h-px bg-border my-0.5" />
          <Button
            type="button"
            variant={isWarning ? 'default' : 'outline'}
            size="sm"
            className={cn(
              'h-7 text-xs gap-1.5',
              isWarning && 'bg-amber-500 hover:bg-amber-600 text-white',
            )}
            onClick={handleCompactClick}
            disabled={isProcessing}
          >
            <Minimize2 className="size-3.5" />
            {isProcessing ? '对话进行中' : '手动压缩'}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  )
}

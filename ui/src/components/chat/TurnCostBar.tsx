/**
 * TurnCostBar - Token 成本计费条
 *
 * 展示每轮对话的 token 使用量和估算成本。
 * 分别显示输入 / 输出 / 缓存 token 和 USD 成本。
 */

import * as React from 'react'
import { Coins, ArrowUp, ArrowDown, Database } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type { TurnCost, ContextStats } from '@/lib/types'

interface TurnCostBarProps {
  turnCost: TurnCost | null
  contextStats?: ContextStats | null
  className?: string
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function formatCost(usd: string | number): string {
  const val = typeof usd === 'string' ? parseFloat(usd) : usd
  if (isNaN(val) || val === 0) return '$0.00'
  if (val < 0.01) return `$${val.toFixed(4)}`
  return `$${val.toFixed(2)}`
}

interface TokenPillProps {
  icon: React.ElementType
  label: string
  value: number
  color: string
}

function TokenPill({ icon: Icon, label, value, color }: TokenPillProps): React.ReactElement {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="flex items-center gap-1 text-xs">
          <Icon className="size-3" style={{ color }} />
          <span className="font-mono tabular-nums">{formatTokens(value)}</span>
        </div>
      </TooltipTrigger>
      <TooltipContent side="top" className="text-xs">
        <p>{label}: {value.toLocaleString()} tokens</p>
      </TooltipContent>
    </Tooltip>
  )
}

export function TurnCostBar({
  turnCost,
  contextStats,
  className,
}: TurnCostBarProps): React.ReactElement | null {
  if (!turnCost) return null

  const { inputTokens, outputTokens, costUsd } = turnCost
  const cacheTokens = contextStats?.compactBufferTokens ?? 0
  const cumulativeInput = contextStats?.cumulativeInputTokens
  const cumulativeOutput = contextStats?.cumulativeOutputTokens

  return (
    <div
      className={cn(
        'flex items-center gap-3 px-3 py-1.5',
        'rounded-md border border-border/50 bg-muted/30',
        'text-xs text-muted-foreground select-none',
        className,
      )}
    >
      {/* 输入 tokens */}
      <TokenPill icon={ArrowUp} label="输入" value={inputTokens} color="#3b82f6" />

      <span className="text-border">|</span>

      {/* 输出 tokens */}
      <TokenPill icon={ArrowDown} label="输出" value={outputTokens} color="#22c55e" />

      {/* 缓存 tokens（如果有） */}
      {cacheTokens > 0 && (
        <>
          <span className="text-border">|</span>
          <TokenPill icon={Database} label="缓存" value={cacheTokens} color="#a855f7" />
        </>
      )}

      {/* 成本 */}
      <span className="text-border">|</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="flex items-center gap-1">
            <Coins className="size-3 text-amber-500" />
            <span className="font-mono tabular-nums font-medium">
              {formatCost(costUsd)}
            </span>
          </div>
        </TooltipTrigger>
        <TooltipContent side="top" className="text-xs space-y-1">
          <p className="font-medium">本轮成本</p>
          <p>输入: {inputTokens.toLocaleString()} tokens</p>
          <p>输出: {outputTokens.toLocaleString()} tokens</p>
          {cumulativeInput != null && cumulativeOutput != null && (
            <>
              <hr className="border-border/50" />
              <p className="font-medium">累计使用</p>
              <p>累计输入: {cumulativeInput.toLocaleString()}</p>
              <p>累计输出: {cumulativeOutput.toLocaleString()}</p>
            </>
          )}
        </TooltipContent>
      </Tooltip>
    </div>
  )
}

/**
 * ContextRing - 上下文环形进度图
 *
 * 用 SVG 环形图可视化当前对话的上下文 token 使用率。
 * 颜色随使用率变化：绿 → 黄 → 红。
 * 支持 hover 显示详细统计。
 */

import * as React from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type { ContextStats } from '@/lib/types'

interface ContextRingProps {
  stats: ContextStats | null
  size?: number
  strokeWidth?: number
  className?: string
  onClick?: () => void
}

function getColor(pct: number): string {
  if (pct < 50) return 'var(--safety-yolo, #22c55e)'
  if (pct < 80) return '#eab308'
  return '#ef4444'
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

export function ContextRing({
  stats,
  size = 36,
  strokeWidth = 3.5,
  className,
  onClick,
}: ContextRingProps): React.ReactElement | null {
  if (!stats) return null

  const { totalTokens, maxTokens, usagePercent } = stats
  const pct = Math.min(usagePercent, 100)
  const radius = (size - strokeWidth) / 2
  const circumference = 2 * Math.PI * radius
  const dashOffset = circumference * (1 - pct / 100)
  const color = getColor(pct)

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          className={cn(
            'relative inline-flex items-center justify-center',
            'cursor-pointer select-none transition-transform hover:scale-105',
            className,
          )}
          style={{ width: size, height: size }}
          aria-label={`上下文使用 ${pct.toFixed(0)}%`}
        >
          <svg width={size} height={size} className="-rotate-90">
            {/* 背景环 */}
            <circle
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              stroke="currentColor"
              strokeWidth={strokeWidth}
              className="text-muted/30"
            />
            {/* 进度环 */}
            <circle
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              stroke={color}
              strokeWidth={strokeWidth}
              strokeLinecap="round"
              strokeDasharray={circumference}
              strokeDashoffset={dashOffset}
              className="transition-[stroke-dashoffset,stroke] duration-500 ease-out"
            />
          </svg>
          <span
            className="absolute inset-0 flex items-center justify-center text-[9px] font-semibold leading-none"
            style={{ color }}
          >
            {pct < 1 ? '<1' : pct.toFixed(0)}
          </span>
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-xs space-y-1">
        <p className="font-medium">上下文使用率 {pct.toFixed(1)}%</p>
        <p className="text-muted-foreground">
          {formatTokens(totalTokens)} / {formatTokens(maxTokens)} tokens
        </p>
        {stats.freeTokens != null && (
          <p className="text-muted-foreground">
            剩余 {formatTokens(stats.freeTokens)} tokens
          </p>
        )}
      </TooltipContent>
    </Tooltip>
  )
}

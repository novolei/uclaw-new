/**
 * ContextStatsModal - 上下文统计详情弹窗
 *
 * 通过 shadcn Dialog 展示当前对话的上下文 token 使用详情，
 * 按消息类型分类显示各项占比和具体数值。
 */

import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import type { ContextStats, ContextStatsCategory } from '@/lib/types'

interface ContextStatsModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  stats: ContextStats | null
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

interface BreakdownItem {
  label: string
  tokens: number
  color: string
}

function buildBreakdown(stats: ContextStats): BreakdownItem[] {
  const items: BreakdownItem[] = []

  if (stats.categories && stats.categories.length > 0) {
    for (const cat of stats.categories) {
      items.push({ label: cat.label, tokens: cat.tokens, color: cat.color })
    }
    return items
  }

  // 根据 ContextStats 详细字段构建
  if (stats.systemPromptTokens != null && stats.systemPromptTokens > 0)
    items.push({ label: '系统提示词', tokens: stats.systemPromptTokens, color: '#a855f7' })
  if (stats.mcpPromptsTokens != null && stats.mcpPromptsTokens > 0)
    items.push({ label: 'MCP 提示词', tokens: stats.mcpPromptsTokens, color: '#6366f1' })
  if (stats.skillsTokens != null && stats.skillsTokens > 0)
    items.push({ label: '技能', tokens: stats.skillsTokens, color: '#0ea5e9' })
  if (stats.messagesTokens != null && stats.messagesTokens > 0)
    items.push({ label: '对话消息', tokens: stats.messagesTokens, color: '#22c55e' })
  if (stats.toolUseTokens != null && stats.toolUseTokens > 0)
    items.push({ label: '工具调用', tokens: stats.toolUseTokens, color: '#f97316' })
  if (stats.compactBufferTokens != null && stats.compactBufferTokens > 0)
    items.push({ label: '压缩缓冲', tokens: stats.compactBufferTokens, color: '#eab308' })
  if (stats.freeTokens != null && stats.freeTokens > 0)
    items.push({ label: '可用空间', tokens: stats.freeTokens, color: '#94a3b8' })

  return items
}

function BarRow({ item, maxTokens }: { item: BreakdownItem; maxTokens: number }): React.ReactElement {
  const pct = maxTokens > 0 ? (item.tokens / maxTokens) * 100 : 0
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <div className="flex items-center gap-2">
          <span
            className="inline-block size-2.5 rounded-full"
            style={{ backgroundColor: item.color }}
          />
          <span className="text-foreground">{item.label}</span>
        </div>
        <span className="font-mono tabular-nums text-muted-foreground">
          {formatTokens(item.tokens)} <span className="opacity-50">({pct.toFixed(1)}%)</span>
        </span>
      </div>
      <div className="h-1.5 rounded-full bg-muted overflow-hidden">
        <div
          className="h-full rounded-full transition-all duration-300"
          style={{ width: `${pct}%`, backgroundColor: item.color }}
        />
      </div>
    </div>
  )
}

export function ContextStatsModal({
  open,
  onOpenChange,
  stats,
}: ContextStatsModalProps): React.ReactElement {
  const breakdown = stats ? buildBreakdown(stats) : []
  const usedTokens = stats ? stats.totalTokens : 0
  const maxTokens = stats ? stats.maxTokens : 0
  const pct = stats ? Math.min(stats.usagePercent, 100) : 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            上下文统计
            <Badge variant="outline" className="font-mono text-xs">
              {pct.toFixed(1)}%
            </Badge>
          </DialogTitle>
          <DialogDescription>
            当前对话上下文 token 使用详情
          </DialogDescription>
        </DialogHeader>

        {!stats ? (
          <p className="text-sm text-muted-foreground py-4 text-center">暂无统计数据</p>
        ) : (
          <div className="space-y-4">
            {/* 总计摘要 */}
            <div className="flex items-baseline justify-between px-1">
              <span className="text-sm text-muted-foreground">总使用 / 上限</span>
              <span className="text-lg font-semibold font-mono tabular-nums">
                {formatTokens(usedTokens)}{' '}
                <span className="text-sm text-muted-foreground font-normal">/ {formatTokens(maxTokens)}</span>
              </span>
            </div>

            {/* 总进度条 */}
            <div className="h-2.5 rounded-full bg-muted overflow-hidden">
              <div
                className={cn(
                  'h-full rounded-full transition-all duration-500',
                  pct < 50 && 'bg-green-500',
                  pct >= 50 && pct < 80 && 'bg-yellow-500',
                  pct >= 80 && 'bg-red-500',
                )}
                style={{ width: `${pct}%` }}
              />
            </div>

            {/* 分类明细 */}
            {breakdown.length > 0 && (
              <div className="space-y-3 pt-2">
                {breakdown.map((item) => (
                  <BarRow key={item.label} item={item} maxTokens={maxTokens} />
                ))}
              </div>
            )}

            {/* 累计 API token */}
            {(stats.cumulativeInputTokens != null || stats.cumulativeOutputTokens != null) && (
              <div className="border-t border-border/50 pt-3 space-y-1">
                <p className="text-xs font-medium text-muted-foreground">累计 API 使用</p>
                <div className="flex gap-4 text-xs">
                  {stats.cumulativeInputTokens != null && (
                    <span>
                      输入: <span className="font-mono">{formatTokens(stats.cumulativeInputTokens)}</span>
                    </span>
                  )}
                  {stats.cumulativeOutputTokens != null && (
                    <span>
                      输出: <span className="font-mono">{formatTokens(stats.cumulativeOutputTokens)}</span>
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

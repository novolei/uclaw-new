import * as React from 'react'
import { cn } from '@/lib/utils'

const TYPE_META: Record<
  string,
  { label: string; tooltip: string; cls: string }
> = {
  automation: {
    label: '数字人',
    tooltip: '自动化数字员工 — 完整的 AI 智能体，能订阅事件、执行任务、记忆历史',
    cls: 'bg-primary/10 text-primary border-primary/30',
  },
  mcp: {
    label: 'MCP',
    tooltip: 'Model Context Protocol 服务 — 给数字人提供工具能力',
    cls: 'bg-primary/10 text-primary border-primary/30',
  },
  skill: {
    label: '技能',
    tooltip: '复用技能脚本 — 装到工作区供多个数字人共享',
    cls: 'bg-success-bg text-success border-success/30',
  },
  extension: {
    label: '扩展',
    tooltip: 'uClaw 应用扩展',
    cls: 'bg-warning-bg text-warning border-warning/30',
  },
}

interface Props {
  type: string
  /** Position of the hover tooltip relative to the badge. */
  tooltipDirection?: 'up' | 'down'
  className?: string
}

export function AppTypeBadge({ type, tooltipDirection = 'down', className }: Props): React.ReactElement {
  const meta = TYPE_META[type] ?? {
    label: type,
    tooltip: `Unknown type: ${type}`,
    cls: 'bg-muted text-muted-foreground border-border',
  }

  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-[1px] rounded-md border text-[10px] font-medium tabular-nums',
        meta.cls,
        className,
      )}
      title={meta.tooltip}
      aria-label={meta.tooltip}
      data-tooltip-direction={tooltipDirection}
    >
      {meta.label}
    </span>
  )
}

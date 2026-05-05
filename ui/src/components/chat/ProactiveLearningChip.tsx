/**
 * ProactiveLearningChip - 主动学习事件指示芯片
 *
 * 在消息气泡下方展示 AI 主动学习事件摘要，
 * 包含场景类型、提取条目数和分类标签。
 */

import * as React from 'react'
import { Brain, BookOpen, Layers } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { ProactiveLearningEvent } from '@/lib/types'
import { cn } from '@/lib/utils'

interface ProactiveLearningChipProps {
  event: ProactiveLearningEvent
  className?: string
}

const SCENARIO_META: Record<
  ProactiveLearningEvent['scenario'],
  { label: string; icon: React.ElementType; color: string }
> = {
  conversation_learning: {
    label: '对话学习',
    icon: Brain,
    color: 'text-violet-500',
  },
  skill_extraction: {
    label: '技能提取',
    icon: BookOpen,
    color: 'text-blue-500',
  },
  multimodal_context: {
    label: '多模态上下文',
    icon: Layers,
    color: 'text-emerald-500',
  },
}

export function ProactiveLearningChip({
  event,
  className,
}: ProactiveLearningChipProps): React.ReactElement {
  const meta = SCENARIO_META[event.scenario] ?? SCENARIO_META.conversation_learning
  const Icon = meta.icon

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className={cn(
            'inline-flex items-center gap-1.5 rounded-full',
            'border border-border/50 bg-muted/40 px-2.5 py-1',
            'text-xs text-muted-foreground cursor-default select-none',
            'transition-colors hover:bg-muted/70',
            className,
          )}
        >
          <Icon className={cn('size-3.5 shrink-0', meta.color)} />
          <span className="font-medium">{meta.label}</span>
          <span className="opacity-60">·</span>
          <span>{event.items_extracted} 条</span>
          {event.categories.length > 0 && (
            <>
              <span className="opacity-60">·</span>
              <div className="flex items-center gap-1">
                {event.categories.slice(0, 3).map((cat) => (
                  <Badge
                    key={cat}
                    variant="secondary"
                    className="px-1.5 py-0 text-[10px] leading-4"
                  >
                    {cat}
                  </Badge>
                ))}
                {event.categories.length > 3 && (
                  <span className="text-[10px] opacity-50">
                    +{event.categories.length - 3}
                  </span>
                )}
              </div>
            </>
          )}
        </div>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-xs">
        <p className="text-xs">{event.summary}</p>
        <p className="text-[10px] text-muted-foreground mt-1">
          {new Date(event.timestamp).toLocaleString()}
        </p>
      </TooltipContent>
    </Tooltip>
  )
}

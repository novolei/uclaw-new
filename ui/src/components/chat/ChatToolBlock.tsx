/**
 * ChatToolBlock — Chat 模式工具调用块
 *
 * 时间线节点样式：
 *   ●─── 🔧 bash ls -a
 *   ●─── ✓ bash rm -rf …
 *
 * 节点（dot）位于左侧时间线主干上，状态以颜色编码：
 *   - 运行中：蓝色 + 脉冲
 *   - 出错：destructive
 *   - 完成：muted
 */

import * as React from 'react'
import { ChevronRight, AlertTriangle, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getToolIcon } from '@/components/agent/tool-utils'
import { getToolPhrase } from '@/components/agent/tool-phrase'
import { ToolResultRenderer } from '@/components/agent/tool-result-renderers'

export interface ChatToolBlockProps {
  toolName: string
  input: Record<string, unknown>
  result?: string
  isError?: boolean
  isCompleted: boolean
  animate?: boolean
  index?: number
}

export function ChatToolBlock({
  toolName,
  input,
  result,
  isError = false,
  isCompleted,
  animate = false,
  index = 0,
}: ChatToolBlockProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)

  const phrase = getToolPhrase(toolName, input)
  const ToolIcon = getToolIcon(toolName)
  const displayLabel = isCompleted ? phrase.label : phrase.loadingLabel

  const delay = animate && index < 10 ? `${index * 30}ms` : '0ms'
  const canExpand = !!result

  return (
    <div
      className={cn(
        'relative',
        animate && 'animate-in fade-in slide-in-from-left-1 duration-200 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      <button
        type="button"
        disabled={!canExpand}
        onClick={() => canExpand && setExpanded((v) => !v)}
        className={cn(
          // 时间线节点 + 行容器：左 padding 留出 22px 给 dot，右侧才是文字
          'group relative flex w-full items-center gap-2 rounded-md py-1 pl-[26px] pr-2 text-left',
          'transition-colors duration-150',
          canExpand
            ? 'hover:bg-foreground/[0.03] dark:hover:bg-foreground/[0.04] cursor-pointer'
            : 'cursor-default',
        )}
      >
        {/* 时间线节点（圆点） */}
        <span
          aria-hidden="true"
          className={cn(
            'absolute left-[6px] top-1/2 -translate-y-1/2 size-[11px] rounded-full',
            'ring-[3px] ring-background dark:ring-background',
            // 状态颜色编码
            !isCompleted
              ? 'bg-primary/80 shadow-[0_0_0_2px_hsl(var(--primary)/0.18)] animate-pulse'
              : isError
                ? 'bg-destructive/80 shadow-[0_0_0_2px_hsl(var(--destructive)/0.18)]'
                : 'bg-muted-foreground/35 group-hover:bg-foreground/55 transition-colors',
          )}
        />

        {/* 状态/加载图标（仅运行中或出错显示）+ 工具图标 */}
        {!isCompleted ? (
          <Loader2 className="size-3 shrink-0 animate-spin text-primary/70" />
        ) : isError ? (
          <AlertTriangle className="size-3 shrink-0 text-destructive/70" />
        ) : null}

        <ToolIcon
          className={cn(
            'size-3.5 shrink-0',
            isError
              ? 'text-destructive/60'
              : 'text-muted-foreground/65 group-hover:text-foreground/75',
          )}
        />

        <span
          className={cn(
            'truncate text-[13.5px] tracking-[-0.005em]',
            isError
              ? 'text-destructive/85'
              : 'text-foreground/70 group-hover:text-foreground/90',
            !isCompleted && 'text-foreground/85',
          )}
        >
          {displayLabel}
        </span>

        {canExpand && (
          <ChevronRight
            className={cn(
              'ml-auto shrink-0 size-3 text-muted-foreground/40',
              'transition-transform duration-150',
              'opacity-0 group-hover:opacity-100',
              expanded && 'rotate-90 opacity-100',
            )}
          />
        )}
      </button>

      {/* 展开面板：缩进对齐文字起始位置，左侧贴合时间线主干 */}
      {expanded && result && (
        <div
          className={cn(
            'ml-[26px] mr-2 mt-1 mb-2 pl-3 pr-1 py-1.5',
            'border-l border-border/50 dark:border-border/60',
            'animate-in fade-in slide-in-from-top-1 duration-150',
          )}
        >
          <ToolResultRenderer
            toolName={toolName}
            input={input}
            result={result}
            isError={isError}
          />
        </div>
      )}
    </div>
  )
}

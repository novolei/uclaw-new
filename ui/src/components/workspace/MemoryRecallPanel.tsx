/**
 * MemoryRecallPanel — 记忆召回可视化面板
 *
 * 监听 agent:memory-recall IPC 事件，在 workspace 右下角以轻量 badge
 * 展示"已召回 N 条记忆，激活 M 个技能"，点击可展开查看简要列表。
 *
 * 每次新的 Agent turn 触发记忆召回时，badge 自动更新。
 * 若无召回事件（totalCandidates === 0），面板不显示。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Brain, Sparkles, ChevronDown, X } from 'lucide-react'
import { motion, AnimatePresence } from 'motion/react'
import { memoryRecallEventAtom, type MemoryRecallEvent } from '@/atoms/agent-atoms'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'

// ─── 常量 ─────────────────────────────────────────────────────────────

/** badge 自动消失时间（ms） */
const AUTO_HIDE_MS = 30_000

/** 每层显示标签 */
const LAYER_LABELS: Record<string, { label: string; color: string }> = {
  boot: { label: 'Boot', color: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20' },
  triggered: { label: 'Triggered', color: 'bg-amber-500/10 text-amber-400 border-amber-500/20' },
  relevant: { label: 'Relevant', color: 'bg-blue-500/10 text-blue-400 border-blue-500/20' },
  expanded: { label: 'Expanded', color: 'bg-purple-500/10 text-purple-400 border-purple-500/20' },
  recent: { label: 'Recent', color: 'bg-zinc-500/10 text-zinc-400 border-zinc-500/20' },
}

/** 按 kind 映射显示标签 */
const KIND_LABELS: Record<string, string> = {
  procedure: '技能',
  user_profile: '偏好',
  episode: '事件',
  knowledge: '知识',
  reference: '参考',
  identity: '身份',
  value: '价值观',
  directive: '指令',
  curated: '精选',
  boot: '引导',
}

// ─── 辅助 ─────────────────────────────────────────────────────────────

function getLayerLabel(item: MemoryRecallEvent['items'][0], event: MemoryRecallEvent): string | null {
  // 根据 item 在哪个层中来标记
  // 这里用 id 匹配简化处理 — 实际按顺序推断
  return null
}

/** 推断 item 所属的召回层 */
function inferItemLayer(
  itemIdx: number,
  event: MemoryRecallEvent,
): string | null {
  const { bootCount, triggeredCount, relevantCount, expandedCount } = event
  if (itemIdx < bootCount) return 'Boot'
  if (itemIdx < bootCount + triggeredCount) return 'Triggered'
  if (itemIdx < bootCount + triggeredCount + relevantCount) return 'Relevant'
  if (itemIdx < bootCount + triggeredCount + relevantCount + expandedCount) return 'Expanded'
  return 'Recent'
}

// ─── 组件 ─────────────────────────────────────────────────────────────

export function MemoryRecallPanel(): React.ReactElement | null {
  const event = useAtomValue(memoryRecallEventAtom)
  const [dismissed, setDismissed] = React.useState(false)
  const [open, setOpen] = React.useState(false)
  const timerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  // 新事件到来时重置 dismissed 状态
  React.useEffect(() => {
    if (event && event.totalCandidates > 0) {
      setDismissed(false)
    }
  }, [event])

  // 自动消失定时器
  React.useEffect(() => {
    if (!event || event.totalCandidates === 0 || dismissed) return

    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      setDismissed(true)
    }, AUTO_HIDE_MS)

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [event, dismissed])

  // 无事件或已消失或 candidates 为 0 时不渲染
  if (!event || event.totalCandidates === 0 || dismissed) return null

  const hasSkills = event.skillsCount > 0
  const layers = [
    { name: 'Boot', count: event.bootCount },
    { name: 'Triggered', count: event.triggeredCount },
    { name: 'Relevant', count: event.relevantCount },
    { name: 'Expanded', count: event.expandedCount },
    { name: 'Recent', count: event.recentCount },
  ].filter((l) => l.count > 0)

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, y: 16, scale: 0.95 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 8, scale: 0.95 }}
        transition={{ duration: 0.25, ease: 'easeOut' }}
        className="fixed bottom-4 right-4 z-50"
      >
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              className={cn(
                'group flex items-center gap-2 rounded-full border px-3 py-1.5',
                'bg-background/95 backdrop-blur-sm shadow-lg',
                'text-xs font-medium text-foreground/80',
                'hover:bg-accent hover:text-foreground transition-colors',
                'cursor-pointer select-none',
              )}
            >
              <Brain className="size-3.5 text-purple-400 shrink-0" />
              <span>
                已召回 <span className="text-foreground font-semibold">{event.totalCandidates}</span> 条记忆
                {hasSkills && (
                  <>
                    ，激活{' '}
                    <span className="text-foreground font-semibold">{event.skillsCount}</span> 个技能
                  </>
                )}
              </span>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation()
                  setDismissed(true)
                }}
                className="ml-0.5 p-0.5 rounded-full hover:bg-muted-foreground/15 opacity-0 group-hover:opacity-100 transition-opacity"
                aria-label="关闭召回面板"
              >
                <X className="size-3" />
              </button>
            </button>
          </PopoverTrigger>
          <PopoverContent
            side="top"
            align="end"
            className="w-80 max-h-96 overflow-y-auto p-0"
          >
            <div className="p-3 border-b">
              <div className="flex items-center gap-2 mb-1">
                <Sparkles className="size-4 text-purple-400" />
                <span className="text-sm font-semibold">记忆召回详情</span>
              </div>
              <p className="text-xs text-muted-foreground">
                共召回 {event.totalCandidates} 条记忆
                {hasSkills && `，含 ${event.skillsCount} 个学得技能`}
              </p>
            </div>

            {/* 层级分布 */}
            {layers.length > 0 && (
              <div className="px-3 py-2 border-b">
                <p className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
                  层级分布
                </p>
                <div className="flex flex-wrap gap-1">
                  {layers.map((l) => (
                    <Badge
                      key={l.name}
                      variant="outline"
                      className={cn('text-[10px] px-1.5 py-0', LAYER_LABELS[l.name.toLowerCase()]?.color)}
                    >
                      {l.name}: {l.count}
                    </Badge>
                  ))}
                </div>
              </div>
            )}

            {/* 记忆列表 */}
            <div className="py-1">
              {event.items.slice(0, 20).map((item, idx) => {
                const layer = inferItemLayer(idx, event)
                const kindLabel = KIND_LABELS[item.kind] ?? item.kind
                return (
                  <div
                    key={item.nodeId || idx}
                    className="flex items-start gap-2 px-3 py-1.5 hover:bg-accent/50 transition-colors"
                  >
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-medium truncate">{item.title}</p>
                      <div className="flex items-center gap-1.5 mt-0.5">
                        {layer && (
                          <span className="text-[10px] text-muted-foreground/60">{layer}</span>
                        )}
                        <span className="text-[10px] text-muted-foreground/50">·</span>
                        <span className="text-[10px] text-muted-foreground/60">{kindLabel}</span>
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          </PopoverContent>
        </Popover>
      </motion.div>
    </AnimatePresence>
  )
}

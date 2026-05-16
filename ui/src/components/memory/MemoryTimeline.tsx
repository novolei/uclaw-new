/**
 * MemoryTimeline — 时间线视图组件。
 *
 * 从 MemoryPanel 的 TimelineView 提取并增强:
 * 日期分组、stagger 渐入动画、kind 配色圆点。
 */
import * as React from 'react'
import { motion } from 'motion/react'
import { Loader2, RefreshCw } from 'lucide-react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn, safeParseDate, formatDateTime } from '@/lib/utils'
import { memoryGraphListTimeline } from '@/lib/tauri-bridge'
import { SUBTYPE_COLORS } from './FragmentCard'
import type { MemoryTimelineEntry, MemoryNodeKind } from '@/lib/types'

// ─── Kind 配色 ──────────────────────────────────────────────────────────

const KIND_COLORS: Record<MemoryNodeKind, string> = {
  boot: '#ef4444',
  identity: '#a855f7',
  value: '#3b82f6',
  user_profile: '#22c55e',
  directive: '#f97316',
  curated: '#0ea5e9',
  episode: '#eab308',
  procedure: '#14b8a6',
  reference: '#6b7280',
}

// Fragment 节点辅助函数
function getFragmentDotClass(entry: MemoryTimelineEntry): string | null {
  if (entry.kind !== 'episode' || entry.metadata?.subtype !== 'fragment') return null
  const tag = entry.metadata?.fragmentTag || 'daily'
  return SUBTYPE_COLORS[tag]?.dot || 'bg-orange-400'
}

function getFragmentBadgeLabel(entry: MemoryTimelineEntry): string | null {
  if (entry.kind !== 'episode' || entry.metadata?.subtype !== 'fragment') return null
  const LABELS: Record<string, string> = {
    daily: '日常', credential: '凭证', location: '位置',
    reminder: '提醒', inspiration: '灵感', bookmark: '书签',
  }
  const tag = entry.metadata?.fragmentTag || 'daily'
  return LABELS[tag] || tag
}

// ─── 日期分组工具 ────────────────────────────────────────────────────────

type DateGroup = '今天' | '昨天' | '本周' | '更早'

function getDateGroup(dateStr: string): DateGroup {
  const date = safeParseDate(dateStr)
  if (!date) return '更早'
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)
  const weekStart = new Date(today)
  weekStart.setDate(weekStart.getDate() - today.getDay())

  if (date >= today) return '今天'
  if (date >= yesterday) return '昨天'
  if (date >= weekStart) return '本周'
  return '更早'
}

function groupByDate(entries: MemoryTimelineEntry[]): { label: DateGroup; items: MemoryTimelineEntry[] }[] {
  const order: DateGroup[] = ['今天', '昨天', '本周', '更早']
  const groups = new Map<DateGroup, MemoryTimelineEntry[]>()

  for (const entry of entries) {
    const group = getDateGroup(entry.updatedAt)
    if (!groups.has(group)) groups.set(group, [])
    groups.get(group)!.push(entry)
  }

  return order
    .filter((label) => groups.has(label))
    .map((label) => ({ label, items: groups.get(label)! }))
}

// ─── Props ──────────────────────────────────────────────────────────────

interface MemoryTimelineProps {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
  className?: string
}

export function MemoryTimeline({ spaceId, onSelectNode, className }: MemoryTimelineProps): React.ReactElement {
  const [entries, setEntries] = React.useState<MemoryTimelineEntry[]>([])
  const [loading, setLoading] = React.useState(true)

  const fetchTimeline = React.useCallback(async () => {
    setLoading(true)
    try {
      const res = await memoryGraphListTimeline({ spaceId, limit: 50 })
      const items = (res as any)?.entries ?? res
      setEntries(Array.isArray(items) ? items : [])
    } catch (err) {
      console.error('[MemoryTimeline] 加载时间线失败:', err)
    } finally {
      setLoading(false)
    }
  }, [spaceId])

  React.useEffect(() => {
    fetchTimeline()
  }, [fetchTimeline])

  // 日期分组
  const grouped = React.useMemo(() => groupByDate(entries), [entries])

  // Loading 状态
  if (loading && entries.length === 0) {
    return (
      <div className={cn('flex items-center justify-center h-full', className)}>
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  // 空状态
  if (entries.length === 0) {
    return (
      <div className={cn('flex flex-col items-center justify-center gap-3 h-full', className)}>
        <p className="text-sm text-muted-foreground">暂无时间线数据</p>
        <Button size="sm" variant="outline" onClick={fetchTimeline}>
          <RefreshCw className="size-3.5 mr-1.5" />
          刷新
        </Button>
      </div>
    )
  }

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* 顶部操作栏 */}
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs font-medium text-muted-foreground">
          最近更新
          <Badge variant="secondary" className="text-[10px] px-1.5 py-0 ml-1.5">
            {entries.length}
          </Badge>
        </span>
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7"
          onClick={fetchTimeline}
          disabled={loading}
        >
          <RefreshCw className={cn('size-3.5', loading && 'animate-spin')} />
        </Button>
      </div>

      {/* 时间线内容 */}
      <ScrollArea className="flex-1">
        <div className="space-y-5 pb-4">
          {grouped.map((group, groupIdx) => (
            <div key={group.label}>
              {/* 日期分组标签 */}
              <div className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground/70 mb-2 pl-5">
                {group.label}
              </div>

              {/* 时间线条目 */}
              <div className="relative pl-5">
                {/* 竖线 */}
                <div className="absolute left-[7px] top-2 bottom-2 w-px bg-border" />

                {group.items.map((entry, idx) => {
                  const isFragment = entry.kind === 'episode' && entry.metadata?.subtype === 'fragment'
                  const color = isFragment ? undefined : (KIND_COLORS[entry.kind] ?? '#6b7280')
                  const fragmentDotClass = isFragment ? getFragmentDotClass(entry) : null
                  const fragmentLabel = isFragment ? getFragmentBadgeLabel(entry) : null
                  const globalIdx = groupIdx * 10 + idx
                  return (
                    <motion.div
                      key={`${entry.nodeId}-${idx}`}
                      initial={{ opacity: 0, x: -8 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ delay: globalIdx * 0.03, duration: 0.3 }}
                      className="relative flex gap-3 py-2 cursor-pointer hover:bg-muted/40 rounded-md px-2 transition-colors"
                      onClick={() => onSelectNode?.(entry.nodeId)}
                    >
                      {/* 时间线圆点 */}
                      <div
                        className={cn(
                          'absolute left-0 top-3.5 size-2.5 rounded-full border-2 border-background z-10 shadow-sm',
                          fragmentDotClass,
                        )}
                        style={color ? { backgroundColor: color } : undefined}
                      />
                      <div className="flex-1 min-w-0 pl-2">
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-medium truncate">{entry.title}</span>
                          {fragmentLabel ? (
                            <Badge
                              variant="outline"
                              className="text-[9px] px-1 py-0 shrink-0 border-orange-400 text-orange-500"
                            >
                              碎片·{fragmentLabel}
                            </Badge>
                          ) : (
                            <Badge
                              variant="outline"
                              className="text-[9px] px-1 py-0 shrink-0"
                              style={{ borderColor: color, color }}
                            >
                              {entry.kind}
                            </Badge>
                          )}
                        </div>
                        {entry.contentSnippet && (
                          <p className="text-[11px] text-muted-foreground line-clamp-1 mt-0.5">
                            {entry.contentSnippet}
                          </p>
                        )}
                        <span className="text-[10px] text-muted-foreground/60">
                        {formatDateTime(entry.updatedAt)}
                        </span>
                      </div>
                    </motion.div>
                  )
                })}
              </div>
            </div>
          ))}
        </div>
      </ScrollArea>
    </div>
  )
}

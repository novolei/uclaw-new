/**
 * MemoryPanel - 主记忆面板
 *
 * 整合 Boot 列表、时间线、搜索入口和记忆图可视化。
 * 使用 Tabs 在不同视图之间切换。
 */

import * as React from 'react'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Network,
  Star,
  Clock,
  Search,
  Loader2,
  RefreshCw,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { memoryGraphListTimeline } from '@/lib/tauri-bridge'
import type { MemoryTimelineEntry, MemoryNodeKind } from '@/lib/types'
import { MemoryBootList } from './MemoryBootList'
import { MemorySearchPanel } from './MemorySearchPanel'
import { MemoryGraphView } from './MemoryGraphView'
import { MemoryNodeCard } from './MemoryNodeCard'

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

// ─── Props ──────────────────────────────────────────────────────────────

interface MemoryPanelProps {
  spaceId?: string
  className?: string
}

export function MemoryPanel({ spaceId, className }: MemoryPanelProps): React.ReactElement {
  const [selectedNodeId, setSelectedNodeId] = React.useState<string | null>(null)
  const [tab, setTab] = React.useState<string>('graph')

  const handleSelectNode = (nodeId: string): void => {
    setSelectedNodeId(nodeId)
  }

  const handleNodeDeleted = (): void => {
    setSelectedNodeId(null)
  }

  return (
    <div className={cn('flex flex-col h-full', className)}>
      <Tabs value={tab} onValueChange={setTab} className="flex flex-col h-full">
        {/* 标签栏 */}
        <div className="flex items-center gap-2 px-3 pt-2 pb-1 border-b border-border/50">
          <TabsList className="h-8 bg-muted/40">
            <TabsTrigger value="graph" className="text-xs gap-1 h-6 px-2.5">
              <Network className="size-3" />
              记忆图
            </TabsTrigger>
            <TabsTrigger value="boot" className="text-xs gap-1 h-6 px-2.5">
              <Star className="size-3" />
              Boot
            </TabsTrigger>
            <TabsTrigger value="timeline" className="text-xs gap-1 h-6 px-2.5">
              <Clock className="size-3" />
              时间线
            </TabsTrigger>
            <TabsTrigger value="search" className="text-xs gap-1 h-6 px-2.5">
              <Search className="size-3" />
              搜索
            </TabsTrigger>
          </TabsList>
        </div>

        {/* 记忆图 */}
        <TabsContent value="graph" className="flex-1 m-0 relative">
          <MemoryGraphView onSelectNode={handleSelectNode} className="h-full" />
        </TabsContent>

        {/* Boot 列表 */}
        <TabsContent value="boot" className="flex-1 m-0 p-3">
          <MemoryBootList
            spaceId={spaceId}
            onSelectNode={handleSelectNode}
            className="h-full"
          />
        </TabsContent>

        {/* 时间线 */}
        <TabsContent value="timeline" className="flex-1 m-0 p-3">
          <TimelineView spaceId={spaceId} onSelectNode={handleSelectNode} />
        </TabsContent>

        {/* 搜索 */}
        <TabsContent value="search" className="flex-1 m-0 p-3">
          <MemorySearchPanel
            spaceId={spaceId}
            onSelectNode={handleSelectNode}
            className="h-full"
          />
        </TabsContent>
      </Tabs>

      {/* 节点详情弹窗 */}
      <Dialog
        open={!!selectedNodeId}
        onOpenChange={(open) => { if (!open) setSelectedNodeId(null) }}
      >
        <DialogContent className="sm:max-w-lg max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>记忆节点详情</DialogTitle>
          </DialogHeader>
          {selectedNodeId && (
            <MemoryNodeCard
              nodeId={selectedNodeId}
              onDeleted={handleNodeDeleted}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}

// ─── Timeline 子组件 ────────────────────────────────────────────────────

function TimelineView({
  spaceId,
  onSelectNode,
}: {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
}): React.ReactElement {
  const [entries, setEntries] = React.useState<MemoryTimelineEntry[]>([])
  const [loading, setLoading] = React.useState(true)

  const fetchTimeline = React.useCallback(async () => {
    setLoading(true)
    try {
      const res = await memoryGraphListTimeline({ spaceId, limit: 50 })
      const items = (res as any)?.entries ?? res
      setEntries(Array.isArray(items) ? items : [])
    } catch (err) {
      console.error('[TimelineView] 加载时间线失败:', err)
    } finally {
      setLoading(false)
    }
  }, [spaceId])

  React.useEffect(() => {
    fetchTimeline()
  }, [fetchTimeline])

  if (loading && entries.length === 0) {
    return (
      <div className="flex items-center justify-center py-10">
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (entries.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 py-10">
        <p className="text-xs text-muted-foreground">暂无时间线数据</p>
        <Button size="sm" variant="outline" className="text-xs" onClick={fetchTimeline}>
          <RefreshCw className="size-3 mr-1" />
          刷新
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-1 mb-2">
        <span className="text-xs font-medium text-muted-foreground">
          最近更新
          <Badge variant="secondary" className="text-[10px] px-1.5 py-0 ml-1.5">
            {entries.length}
          </Badge>
        </span>
        <Button
          size="icon"
          variant="ghost"
          className="h-6 w-6"
          onClick={fetchTimeline}
          disabled={loading}
        >
          <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
        </Button>
      </div>

      <ScrollArea className="flex-1">
        <div className="relative pl-4 space-y-0">
          {/* 竖线 */}
          <div className="absolute left-[7px] top-2 bottom-2 w-px bg-border" />

          {entries.map((entry, idx) => {
            const color = KIND_COLORS[entry.kind] ?? '#6b7280'
            return (
              <div
                key={`${entry.nodeId}-${idx}`}
                className="relative flex gap-3 py-2 cursor-pointer hover:bg-muted/40 rounded-md px-2 transition-colors"
                onClick={() => onSelectNode?.(entry.nodeId)}
              >
                {/* 时间线圆点 */}
                <div
                  className="absolute left-0 top-3.5 size-2.5 rounded-full border-2 border-background z-10"
                  style={{ backgroundColor: color }}
                />
                <div className="flex-1 min-w-0 pl-2">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium truncate">{entry.title}</span>
                    <Badge variant="outline" className="text-[9px] px-1 py-0 shrink-0" style={{ borderColor: color, color }}>
                      {entry.kind}
                    </Badge>
                  </div>
                  <p className="text-[11px] text-muted-foreground line-clamp-1 mt-0.5">
                    {entry.contentSnippet}
                  </p>
                  <span className="text-[10px] text-muted-foreground/60">
                    {new Date(entry.updatedAt).toLocaleString()}
                  </span>
                </div>
              </div>
            )
          })}
        </div>
      </ScrollArea>
    </div>
  )
}

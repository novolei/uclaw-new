/**
 * MemoryBootList - Boot 节点列表
 *
 * 展示标记为 "boot" 的记忆节点列表，支持管理（移除 boot 标记）。
 */

import * as React from 'react'
import { Star, Loader2, RefreshCw, Trash2 } from 'lucide-react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn, formatDate } from '@/lib/utils'
import { memoryGraphListBoot, memoryGraphManageBoot } from '@/lib/tauri-bridge'
import type { MemoryNode, MemoryNodeKind } from '@/lib/types'

// ─── Kind 颜色映射 ──────────────────────────────────────────────────────

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

interface MemoryBootListProps {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
  className?: string
}

export function MemoryBootList({
  spaceId,
  onSelectNode,
  className,
}: MemoryBootListProps): React.ReactElement {
  const [nodes, setNodes] = React.useState<MemoryNode[]>([])
  const [loading, setLoading] = React.useState(true)

  const fetchBoot = React.useCallback(async () => {
    setLoading(true)
    try {
      const result = await memoryGraphListBoot({ spaceId, limit: 50 })
      // 后端返回 MemoryNodeDetail[]，每个元素有 node/activeVersion 嵌套结构
      const raw = (result as any)?.nodes ?? result
      const mapped = Array.isArray(raw)
        ? raw.map((item: any) => item.node ?? item)
        : []
      setNodes(mapped as MemoryNode[])
    } catch (err) {
      console.error('[MemoryBootList] 加载失败:', err)
    } finally {
      setLoading(false)
    }
  }, [spaceId])

  React.useEffect(() => {
    fetchBoot()
  }, [fetchBoot])

  const removeBoot = async (nodeId: string): Promise<void> => {
    try {
      await memoryGraphManageBoot({ nodeId, action: 'remove', spaceId })
      setNodes((prev) => prev.filter((n) => n.id !== nodeId))
    } catch (err) {
      console.error('[MemoryBootList] 移除 boot 失败:', err)
    }
  }

  return (
    <div className={cn('flex flex-col', className)}>
      {/* Header */}
      <div className="flex items-center justify-between px-1 mb-2">
        <div className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          <Star className="size-3.5 text-amber-500" />
          Boot 节点
          <Badge variant="secondary" className="text-[10px] px-1.5 py-0 ml-1">
            {nodes.length}
          </Badge>
        </div>
        <Button
          size="icon"
          variant="ghost"
          className="h-6 w-6"
          onClick={fetchBoot}
          disabled={loading}
        >
          <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
        </Button>
      </div>

      {/* 列表 */}
      {loading && nodes.length === 0 ? (
        <div className="flex items-center justify-center py-6">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      ) : nodes.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-8 gap-3">
          <div className="flex items-center justify-center size-16 rounded-xl bg-muted/40">
            <Star className="size-8 text-muted-foreground/50" />
          </div>
          <div className="text-center space-y-1">
            <p className="text-xs font-medium text-muted-foreground">暂无 Boot 节点</p>
            <p className="text-[10px] text-muted-foreground/70 max-w-[200px]">
              在记忆节点详情中点击星标按钮将其加入启动列表
            </p>
          </div>
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <div className="space-y-1.5 px-0.5">
            {nodes.map((node) => (
              <div
                key={node.id}
                className={cn(
                  'group flex items-center gap-2.5 rounded-lg border border-border/40 px-2.5 py-2',
                  'hover:translate-y-[-1px] hover:shadow-sm cursor-pointer',
                  'transition-all duration-200',
                )}
                onClick={() => onSelectNode?.(node.id)}
              >
                {/* Kind 颜色圆点 */}
                <span
                  className="size-2.5 rounded-full shrink-0"
                  style={{ backgroundColor: KIND_COLORS[node.kind] ?? '#6b7280' }}
                />

                {/* 中间内容 */}
                <div className="flex-1 min-w-0">
                  <span className="text-xs font-medium truncate block">{node.title}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {formatDate(node.createdAt)}
                  </span>
                </div>

                {/* 右侧操作 */}
                <Star className="size-3 text-amber-500 shrink-0" />
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-5 w-5 opacity-0 group-hover:opacity-100 transition-opacity duration-200 shrink-0"
                      onClick={(e) => {
                        e.stopPropagation()
                        removeBoot(node.id)
                      }}
                    >
                      <Trash2 className="size-3 text-destructive" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    <p className="text-xs">移出 Boot 列表</p>
                  </TooltipContent>
                </Tooltip>
              </div>
            ))}
          </div>
        </ScrollArea>
      )}
    </div>
  )
}

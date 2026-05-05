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
import { cn } from '@/lib/utils'
import { memoryGraphListBoot, memoryGraphManageBoot } from '@/lib/tauri-bridge'
import type { MemoryNode } from '@/lib/types'

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
      setNodes((result as any)?.nodes ?? result as MemoryNode[] ?? [])
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
      {/* 标题行 */}
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
        <p className="text-xs text-muted-foreground text-center py-6">
          暂无 Boot 节点
        </p>
      ) : (
        <ScrollArea className="flex-1">
          <div className="space-y-1">
            {nodes.map((node) => (
              <div
                key={node.id}
                className={cn(
                  'group flex items-center gap-2 rounded-md px-2 py-1.5',
                  'hover:bg-muted/60 cursor-pointer transition-colors',
                )}
                onClick={() => onSelectNode?.(node.id)}
              >
                <Star className="size-3 text-amber-500 shrink-0" />
                <span className="flex-1 text-xs truncate">{node.title}</span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-5 w-5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
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

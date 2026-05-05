/**
 * MemoryNodeCard - 记忆节点详情卡片
 *
 * 展示单个记忆节点的详细信息：标题、类型、内容、关键词、路由、版本历史。
 * 支持编辑和删除操作。
 */

import * as React from 'react'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Input } from '@/components/ui/input'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  Pencil,
  Trash2,
  Save,
  X,
  Clock,
  Tag,
  GitBranch,
  MapPin,
  Star,
  ChevronDown,
  ChevronUp,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  memoryGraphGetNode,
  memoryGraphUpdateNode,
  memoryGraphDeleteNode,
  memoryGraphManageBoot,
} from '@/lib/tauri-bridge'
import type {
  MemoryNode,
  MemoryNodeDetail,
  MemoryNodeKind,
  MemoryVersion,
  MemoryRoute,
} from '@/lib/types'

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

const KIND_LABELS: Record<MemoryNodeKind, string> = {
  boot: '启动',
  identity: '身份',
  value: '价值观',
  user_profile: '用户画像',
  directive: '指令',
  curated: '精选',
  episode: '片段',
  procedure: '流程',
  reference: '参考',
}

// ─── Props ──────────────────────────────────────────────────────────────

interface MemoryNodeCardProps {
  nodeId: string
  /** 外部传入初始数据，避免重复请求 */
  initialDetail?: MemoryNodeDetail
  /** 删除后回调 */
  onDeleted?: (nodeId: string) => void
  /** 更新后回调 */
  onUpdated?: (node: MemoryNode) => void
  className?: string
}

export function MemoryNodeCard({
  nodeId,
  initialDetail,
  onDeleted,
  onUpdated,
  className,
}: MemoryNodeCardProps): React.ReactElement {
  const [detail, setDetail] = React.useState<MemoryNodeDetail | null>(initialDetail ?? null)
  const [loading, setLoading] = React.useState(!initialDetail)
  const [editing, setEditing] = React.useState(false)
  const [editTitle, setEditTitle] = React.useState('')
  const [showVersions, setShowVersions] = React.useState(false)

  // 获取详情
  React.useEffect(() => {
    if (initialDetail) return
    setLoading(true)
    memoryGraphGetNode({ nodeId })
      .then((res) => setDetail(res as MemoryNodeDetail))
      .catch((err) => console.error('[MemoryNodeCard] 获取节点详情失败:', err))
      .finally(() => setLoading(false))
  }, [nodeId, initialDetail])

  const node = detail?.node
  const activeVersion = detail?.activeVersion
  const allVersions = detail?.allVersions ?? []
  const routes = detail?.routes ?? []
  const keywords = detail?.keywords ?? []

  const startEdit = (): void => {
    if (!node) return
    setEditTitle(node.title)
    setEditing(true)
  }

  const saveEdit = async (): Promise<void> => {
    if (!node) return
    const trimmed = editTitle.trim()
    if (!trimmed || trimmed === node.title) {
      setEditing(false)
      return
    }
    try {
      await memoryGraphUpdateNode({ nodeId: node.id, title: trimmed })
      const updatedNode = { ...node, title: trimmed }
      setDetail((prev) => prev ? { ...prev, node: updatedNode } : prev)
      onUpdated?.(updatedNode)
    } catch (err) {
      console.error('[MemoryNodeCard] 更新失败:', err)
    }
    setEditing(false)
  }

  const handleDelete = async (): Promise<void> => {
    if (!node) return
    try {
      await memoryGraphDeleteNode({ nodeId: node.id })
      onDeleted?.(node.id)
    } catch (err) {
      console.error('[MemoryNodeCard] 删除失败:', err)
    }
  }

  const toggleBoot = async (): Promise<void> => {
    if (!node) return
    const isBoot = node.kind === 'boot'
    try {
      await memoryGraphManageBoot({
        nodeId: node.id,
        action: isBoot ? 'remove' : 'add',
        spaceId: node.spaceId,
      })
      const updatedNode = { ...node, kind: (isBoot ? 'curated' : 'boot') as MemoryNodeKind }
      setDetail((prev) => prev ? { ...prev, node: updatedNode } : prev)
      onUpdated?.(updatedNode)
    } catch (err) {
      console.error('[MemoryNodeCard] 管理 Boot 失败:', err)
    }
  }

  if (loading) {
    return (
      <Card className={cn('animate-pulse', className)}>
        <CardHeader>
          <div className="h-4 w-2/3 bg-muted rounded" />
          <div className="h-3 w-1/3 bg-muted rounded mt-2" />
        </CardHeader>
        <CardContent>
          <div className="h-20 bg-muted rounded" />
        </CardContent>
      </Card>
    )
  }

  if (!node) {
    return (
      <Card className={className}>
        <CardContent className="py-8 text-center text-sm text-muted-foreground">
          节点不存在或加载失败
        </CardContent>
      </Card>
    )
  }

  const kindColor = KIND_COLORS[node.kind] ?? '#6b7280'
  const kindLabel = KIND_LABELS[node.kind] ?? node.kind

  return (
    <Card className={cn('overflow-hidden', className)}>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-2">
          {editing ? (
            <div className="flex items-center gap-1.5 flex-1 min-w-0">
              <Input
                value={editTitle}
                onChange={(e) => setEditTitle(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') saveEdit()
                  if (e.key === 'Escape') setEditing(false)
                }}
                className="h-7 text-sm"
                autoFocus
              />
              <Button size="icon" variant="ghost" className="h-7 w-7 shrink-0" onClick={saveEdit}>
                <Save className="size-3.5" />
              </Button>
              <Button size="icon" variant="ghost" className="h-7 w-7 shrink-0" onClick={() => setEditing(false)}>
                <X className="size-3.5" />
              </Button>
            </div>
          ) : (
            <CardTitle className="text-sm font-medium leading-tight">{node.title}</CardTitle>
          )}

          {!editing && (
            <div className="flex items-center gap-0.5 shrink-0">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button size="icon" variant="ghost" className="h-6 w-6" onClick={toggleBoot}>
                    <Star className={cn('size-3.5', node.kind === 'boot' && 'fill-amber-500 text-amber-500')} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">
                  <p className="text-xs">{node.kind === 'boot' ? '移出启动列表' : '加入启动列表'}</p>
                </TooltipContent>
              </Tooltip>
              <Button size="icon" variant="ghost" className="h-6 w-6" onClick={startEdit}>
                <Pencil className="size-3.5" />
              </Button>
              <Button size="icon" variant="ghost" className="h-6 w-6 text-destructive" onClick={handleDelete}>
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          )}
        </div>

        <CardDescription className="flex items-center gap-2 mt-1">
          <Badge
            variant="outline"
            className="text-[10px] px-1.5 py-0"
            style={{ borderColor: kindColor, color: kindColor }}
          >
            {kindLabel}
          </Badge>
          <span className="text-[10px] text-muted-foreground flex items-center gap-1">
            <Clock className="size-3" />
            {new Date(node.updatedAt).toLocaleDateString()}
          </span>
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-3 pt-0">
        {/* 内容 */}
        {activeVersion && (
          <div className="text-xs leading-relaxed text-foreground/80 bg-muted/40 rounded-md p-2.5 whitespace-pre-wrap">
            {activeVersion.content}
          </div>
        )}

        {/* 关键词 */}
        {keywords.length > 0 && (
          <div className="flex items-start gap-1.5 flex-wrap">
            <Tag className="size-3 text-muted-foreground mt-0.5 shrink-0" />
            {keywords.map((kw) => (
              <Badge key={kw} variant="secondary" className="text-[10px] px-1.5 py-0">
                {kw}
              </Badge>
            ))}
          </div>
        )}

        {/* 路由 */}
        {routes.length > 0 && (
          <div className="flex items-start gap-1.5 flex-wrap">
            <MapPin className="size-3 text-muted-foreground mt-0.5 shrink-0" />
            {routes.map((r) => (
              <span key={r.id} className="text-[10px] text-muted-foreground font-mono">
                {r.domain}/{r.path}
                {r.isPrimary && <span className="text-primary ml-0.5">★</span>}
              </span>
            ))}
          </div>
        )}

        {/* 版本历史 */}
        {allVersions.length > 1 && (
          <div>
            <button
              type="button"
              onClick={() => setShowVersions(!showVersions)}
              className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
            >
              <GitBranch className="size-3" />
              {allVersions.length} 个版本
              {showVersions ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />}
            </button>
            {showVersions && (
              <ScrollArea className="max-h-32 mt-2">
                <div className="space-y-1.5">
                  {allVersions.map((v) => (
                    <div
                      key={v.id}
                      className={cn(
                        'text-[10px] rounded px-2 py-1 border',
                        v.status === 'active'
                          ? 'border-primary/30 bg-primary/5'
                          : 'border-border/50 bg-muted/30',
                      )}
                    >
                      <div className="flex items-center justify-between">
                        <Badge
                          variant="outline"
                          className="text-[9px] px-1 py-0"
                        >
                          {v.status}
                        </Badge>
                        <span className="text-muted-foreground">
                          {new Date(v.createdAt).toLocaleDateString()}
                        </span>
                      </div>
                      <p className="mt-1 line-clamp-2 text-muted-foreground">{v.content}</p>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

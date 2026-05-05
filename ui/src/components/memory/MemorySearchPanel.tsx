/**
 * MemorySearchPanel - 记忆搜索面板
 *
 * 提供搜索输入框和结果列表，调用 memoryGraphSearch 和 memoryGraphExplainRecall。
 */

import * as React from 'react'
import { Search, Loader2, Sparkles, X } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { cn } from '@/lib/utils'
import { memoryGraphSearch, memoryGraphExplainRecall } from '@/lib/tauri-bridge'
import type { MemoryRecallCandidate, MemoryRecallPlan } from '@/lib/types'

interface MemorySearchPanelProps {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
  className?: string
}

export function MemorySearchPanel({
  spaceId,
  onSelectNode,
  className,
}: MemorySearchPanelProps): React.ReactElement {
  const [query, setQuery] = React.useState('')
  const [results, setResults] = React.useState<MemoryRecallCandidate[]>([])
  const [recallPlan, setRecallPlan] = React.useState<MemoryRecallPlan | null>(null)
  const [searching, setSearching] = React.useState(false)
  const [explaining, setExplaining] = React.useState(false)
  const inputRef = React.useRef<HTMLInputElement>(null)

  const doSearch = async (): Promise<void> => {
    const q = query.trim()
    if (!q) return
    setSearching(true)
    setRecallPlan(null)
    try {
      const res = await memoryGraphSearch({ query: q, spaceId })
      const candidates = (res as any)?.candidates ?? (res as any)?.results ?? res
      setResults(Array.isArray(candidates) ? candidates : [])
    } catch (err) {
      console.error('[MemorySearchPanel] 搜索失败:', err)
      setResults([])
    } finally {
      setSearching(false)
    }
  }

  const doExplainRecall = async (): Promise<void> => {
    const q = query.trim()
    if (!q) return
    setExplaining(true)
    try {
      const plan = await memoryGraphExplainRecall({ query: q, spaceId })
      setRecallPlan(plan as MemoryRecallPlan)
    } catch (err) {
      console.error('[MemorySearchPanel] 召回解释失败:', err)
    } finally {
      setExplaining(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      e.preventDefault()
      doSearch()
    }
  }

  const clearSearch = (): void => {
    setQuery('')
    setResults([])
    setRecallPlan(null)
    inputRef.current?.focus()
  }

  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {/* 搜索栏 */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="搜索记忆..."
            className="h-8 pl-8 pr-8 text-xs"
          />
          {query && (
            <button
              type="button"
              onClick={clearSearch}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <Button size="sm" className="h-8 text-xs" onClick={doSearch} disabled={searching || !query.trim()}>
          {searching ? <Loader2 className="size-3.5 animate-spin" /> : '搜索'}
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-8 text-xs gap-1"
          onClick={doExplainRecall}
          disabled={explaining || !query.trim()}
        >
          <Sparkles className="size-3" />
          {explaining ? '分析中...' : '召回解释'}
        </Button>
      </div>

      {/* 搜索结果 */}
      <ScrollArea className="flex-1">
        {results.length > 0 && (
          <div className="space-y-2">
            <p className="text-[10px] text-muted-foreground px-1">
              找到 {results.length} 条结果
            </p>
            {results.map((item) => (
              <Card
                key={item.nodeId}
                className="cursor-pointer hover:bg-muted/40 transition-colors"
                onClick={() => onSelectNode?.(item.nodeId)}
              >
                <CardContent className="p-2.5 space-y-1.5">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium truncate flex-1">{item.title}</span>
                    <Badge variant="outline" className="text-[9px] px-1 py-0 shrink-0">
                      {item.kind}
                    </Badge>
                    {item.score != null && (
                      <span className="text-[9px] font-mono text-muted-foreground">
                        {(item.score * 100).toFixed(0)}%
                      </span>
                    )}
                  </div>
                  <p className="text-[11px] text-muted-foreground line-clamp-2">{item.content}</p>
                  {item.matchedKeywords.length > 0 && (
                    <div className="flex items-center gap-1 flex-wrap">
                      {item.matchedKeywords.map((kw) => (
                        <Badge key={kw} variant="secondary" className="text-[9px] px-1 py-0">
                          {kw}
                        </Badge>
                      ))}
                    </div>
                  )}
                  {item.reason && (
                    <p className="text-[10px] text-muted-foreground/70 italic">{item.reason}</p>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        )}

        {/* 召回解释 */}
        {recallPlan && (
          <div className="space-y-3 mt-3">
            <p className="text-xs font-medium text-muted-foreground px-1">召回计划</p>
            <RecallSection title="Boot" items={recallPlan.boot} onSelect={onSelectNode} />
            <RecallSection title="触发" items={recallPlan.triggered} onSelect={onSelectNode} />
            <RecallSection title="相关" items={recallPlan.relevant} onSelect={onSelectNode} />
            <RecallSection title="扩展" items={recallPlan.expanded} onSelect={onSelectNode} />
          </div>
        )}

        {/* 空状态 */}
        {!searching && results.length === 0 && !recallPlan && query.trim() && (
          <p className="text-xs text-muted-foreground text-center py-6">
            未找到匹配的记忆
          </p>
        )}
      </ScrollArea>
    </div>
  )
}

// ─── 召回分组子组件 ──────────────────────────────────────────────────────

function RecallSection({
  title,
  items,
  onSelect,
}: {
  title: string
  items: MemoryRecallCandidate[]
  onSelect?: (nodeId: string) => void
}): React.ReactElement | null {
  if (!items || items.length === 0) return null
  return (
    <div className="space-y-1">
      <p className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider px-1">
        {title} ({items.length})
      </p>
      {items.map((item) => (
        <div
          key={item.nodeId}
          className="flex items-center gap-2 rounded px-2 py-1 hover:bg-muted/40 cursor-pointer text-xs transition-colors"
          onClick={() => onSelect?.(item.nodeId)}
        >
          <span className="flex-1 truncate">{item.title}</span>
          <Badge variant="outline" className="text-[9px] px-1 py-0 shrink-0">
            {item.source}
          </Badge>
        </div>
      ))}
    </div>
  )
}

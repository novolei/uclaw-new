/**
 * MemorySearchPanel - 记忆搜索面板
 *
 * 提供搜索输入框和结果列表，调用 memoryGraphSearch 和 memoryGraphExplainRecall。
 */

import * as React from 'react'
import { Search, Loader2, Sparkles, X, Star, Zap, Link2, Maximize2 } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
import { memoryGraphSearch, memoryGraphExplainRecall } from '@/lib/tauri-bridge'
import type { MemoryRecallCandidate, MemoryRecallPlan, MemoryNodeKind } from '@/lib/types'

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

interface MemorySearchPanelProps {
  spaceId?: string
  onSelectNode?: (nodeId: string) => void
  className?: string
}

// ─── 防抖工具 ──────────────────────────────────────────────────────────

function debounce<T extends (...args: any[]) => any>(fn: T, delay: number): (...args: Parameters<T>) => void {
  let timer: ReturnType<typeof setTimeout>
  return (...args: Parameters<T>) => {
    clearTimeout(timer)
    timer = setTimeout(() => fn(...args), delay)
  }
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
  const abortRef = React.useRef<AbortController | null>(null)

  const performSearch = async (q: string, signal: AbortSignal): Promise<void> => {
    if (signal.aborted) return
    setSearching(true)
    setRecallPlan(null)
    try {
      const res = await memoryGraphSearch({ query: q, spaceId })
      if (signal.aborted) return
      // res is MemoryRecallPlan with boot / triggered / relevant / expanded / recent layers.
      // Flatten all layers into a single candidates array for display.
      const plan = res as MemoryRecallPlan
      const allCandidates: MemoryRecallCandidate[] = [
        ...(plan.boot || []),
        ...(plan.triggered || []),
        ...(plan.relevant || []),
        ...(plan.expanded || []),
      ]
      setResults(allCandidates)
    } catch (err) {
      if (!signal.aborted) {
        console.error('[MemorySearchPanel] 搜索失败:', err)
        setResults([])
      }
    } finally {
      if (!signal.aborted) {
        setSearching(false)
      }
    }
  }

  const triggerSearch = React.useCallback((q: string) => {
    const trimmed = q.trim()
    if (!trimmed) return
    abortRef.current?.abort()
    const controller = new AbortController()
    abortRef.current = controller
    performSearch(trimmed, controller.signal)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId])

  const debouncedSearch = React.useMemo(
    () => debounce((q: string) => triggerSearch(q), 300),
    [triggerSearch],
  )

  // 组件卸载时取消正在进行的请求
  React.useEffect(() => {
    return () => { abortRef.current?.abort() }
  }, [])

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
      triggerSearch(query)
    }
  }

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
    const value = e.target.value
    setQuery(value)
    if (value.trim()) {
      debouncedSearch(value)
    }
  }

  const clearSearch = (): void => {
    abortRef.current?.abort()
    setQuery('')
    setResults([])
    setRecallPlan(null)
    setSearching(false)
    inputRef.current?.focus()
  }

  const hasContent = results.length > 0 || recallPlan != null
  const showEmptyInitial = !searching && !hasContent && !query.trim()
  const showNoResults = !searching && results.length === 0 && !recallPlan && query.trim()

  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {/* 搜索栏 */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
          <Input
            ref={inputRef}
            value={query}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            placeholder="搜索记忆..."
            className="h-8 pl-8 pr-16 text-xs"
          />
          <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
            {query ? (
              <button
                type="button"
                onClick={clearSearch}
                className="text-muted-foreground hover:text-foreground cursor-pointer transition-colors duration-150"
              >
                <X className="size-3.5" />
              </button>
            ) : (
              <span className="text-[9px] text-muted-foreground/60 bg-muted/60 rounded px-1.5 py-0.5 font-mono">
                Enter
              </span>
            )}
          </div>
        </div>
        <Button size="sm" className="h-8 text-xs" onClick={() => triggerSearch(query)} disabled={searching || !query.trim()}>
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
        {/* 空搜索初始状态 */}
        {showEmptyInitial && (
          <div className="flex flex-col items-center justify-center py-10 gap-3">
            <div className="flex items-center justify-center size-16 rounded-xl bg-muted/40">
              <Search className="size-8 text-muted-foreground/50" />
            </div>
            <div className="text-center space-y-1">
              <p className="text-xs font-medium text-muted-foreground">搜索记忆节点</p>
              <p className="text-[10px] text-muted-foreground/70 max-w-[220px]">
                输入关键词搜索标题、内容和关联信息
              </p>
            </div>
          </div>
        )}

        {results.length > 0 && (
          <div className="space-y-2">
            <p className="text-[10px] text-muted-foreground px-1">
              找到 {results.length} 条结果
            </p>
            {results.map((item) => (
              <Card
                key={item.nodeId}
                className="cursor-pointer hover:bg-muted/60 hover:shadow-sm transition-all duration-200 overflow-hidden"
                style={{ borderLeftWidth: '4px', borderLeftColor: KIND_COLORS[item.kind] ?? '#6b7280' }}
                onClick={() => onSelectNode?.(item.nodeId)}
              >
                <CardContent className="p-2.5 space-y-1.5">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium truncate flex-1">{item.title}</span>
                    <Badge variant="outline" className="text-[9px] px-1 py-0 shrink-0">
                      {item.kind}
                    </Badge>
                    {item.score != null && (
                      <div className="flex items-center gap-1.5 shrink-0">
                        <div className="w-10 h-1 rounded-full bg-muted overflow-hidden">
                          <div
                            className="h-full rounded-full bg-primary"
                            style={{ width: `${Math.round(item.score * 100)}%` }}
                          />
                        </div>
                        <span className="text-[9px] font-mono text-muted-foreground">
                          {(item.score * 100).toFixed(0)}%
                        </span>
                      </div>
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
            <RecallSection title="Boot" icon={<Star className="size-3.5" />} items={recallPlan.boot} onSelect={onSelectNode} />
            <RecallSection title="触发" icon={<Zap className="size-3.5" />} items={recallPlan.triggered} onSelect={onSelectNode} />
            <RecallSection title="相关" icon={<Link2 className="size-3.5" />} items={recallPlan.relevant} onSelect={onSelectNode} />
            <RecallSection title="扩展" icon={<Maximize2 className="size-3.5" />} items={recallPlan.expanded} onSelect={onSelectNode} />
          </div>
        )}

        {/* 无结果状态 */}
        {showNoResults && (
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
  icon,
  items,
  onSelect,
}: {
  title: string
  icon: React.ReactNode
  items: MemoryRecallCandidate[]
  onSelect?: (nodeId: string) => void
}): React.ReactElement | null {
  const [open, setOpen] = React.useState(true)

  if (!items || items.length === 0) return null

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex items-center gap-1.5 w-full text-left px-1 py-0.5 rounded hover:bg-muted/40 cursor-pointer transition-colors duration-150">
        <span className="text-muted-foreground">{icon}</span>
        <span className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider flex-1">
          {title} ({items.length})
        </span>
        <svg
          className={cn('size-3 text-muted-foreground transition-transform duration-200', open && 'rotate-180')}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="space-y-0.5 mt-1">
          {items.map((item) => (
            <div
              key={item.nodeId}
              className="flex items-center gap-2 rounded px-2 py-1.5 hover:bg-muted/40 cursor-pointer text-xs transition-colors duration-150"
              onClick={() => onSelect?.(item.nodeId)}
            >
              <span className="flex-1 truncate">{item.title}</span>
              <Badge variant="outline" className="text-[9px] px-1 py-0 shrink-0">
                {item.source}
              </Badge>
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
